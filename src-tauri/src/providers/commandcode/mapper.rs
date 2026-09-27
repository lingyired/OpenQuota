use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{QuotaFormat, QuotaWindow};

use super::{client::BillingResponse, CommandCodeError};

const FIVE_HOUR_PERIOD_SECONDS: u64 = 5 * 60 * 60;
const WEEKLY_PERIOD_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Plan catalog keyed by the `planId` the subscriptions endpoint reports. A
/// monthly plan's price is its credit grant, so the monthly window is counted in
/// dollars: used = grant − monthlyCredits left. An unknown plan falls back to a
/// percentage of the remaining credits without totals.
const PLANS: &[(&str, f64, &str)] = &[
    ("individual-go", 10.0, "Go"),
    ("individual-goat", 70.0, "GOAT"),
    ("individual-pro", 80.0, "Pro"),
    ("individual-max", 150.0, "Max"),
    ("individual-ultra", 300.0, "Ultra"),
];

pub(super) struct Billing {
    pub(super) quotas: Vec<QuotaWindow>,
    pub(super) plan: Option<String>,
}

impl std::fmt::Debug for Billing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Billing")
            .field("quotas", &self.quotas)
            .field("plan", &self.plan)
            .finish()
    }
}

pub(super) fn map_billing(
    credits: BillingResponse,
    subscription: Option<BillingResponse>,
) -> Result<Billing, CommandCodeError> {
    check_status(&credits)?;
    let body = credits.body;
    let grant = subscription.as_ref().map(plan_grant).unwrap_or(None);
    let resets_at = subscription.as_ref().and_then(period_end);
    let monthly = monthly_quota(
        number(body.pointer("/credits/monthlyCredits")),
        grant,
        resets_at,
    )?;
    let mut quotas = vec![monthly];
    quotas.extend(map_window(
        body.get("windowLimits"),
        "fiveHour",
        "session",
        "Session (5h)",
        FIVE_HOUR_PERIOD_SECONDS,
    ));
    quotas.extend(map_window(
        body.get("windowLimits"),
        "weekly",
        "weekly",
        "Weekly",
        WEEKLY_PERIOD_SECONDS,
    ));
    if quotas.len() == 1 && body.pointer("/credits/monthlyCredits").is_none() {
        return Err(CommandCodeError::InvalidResponse);
    }
    Ok(Billing {
        quotas,
        plan: subscription.as_ref().and_then(plan_name).map(str::to_owned),
    })
}

fn check_status(response: &BillingResponse) -> Result<(), CommandCodeError> {
    match response.status.as_u16() {
        200..=299 => Ok(()),
        401 | 403 => Err(CommandCodeError::InvalidKey),
        429 => Err(CommandCodeError::RequestFailed(429)),
        status => Err(CommandCodeError::RequestFailed(status)),
    }
}

/// Reads `success.data` from the subscription payload; `None` on any shape
/// mismatch so the optional endpoint never blocks the main quota report.
fn subscription_data(response: &BillingResponse) -> Option<&Value> {
    (response.status.is_success()
        && response.body.get("success").and_then(Value::as_bool) != Some(false))
    .then(|| response.body.get("data"))
    .and_then(|data| data.filter(|data| data.is_object()))
}

fn plan_grant(response: &BillingResponse) -> Option<f64> {
    let plan_id = plan_id(response)?;
    PLANS
        .iter()
        .find(|(id, _, _)| id.eq_ignore_ascii_case(&plan_id))
        .map(|(_, grant, _)| *grant)
}

fn plan_name(response: &BillingResponse) -> Option<&'static str> {
    let plan_id = plan_id(response)?;
    PLANS
        .iter()
        .find(|(id, _, _)| id.eq_ignore_ascii_case(&plan_id))
        .map(|(_, _, name)| *name)
}

fn plan_id(response: &BillingResponse) -> Option<String> {
    subscription_data(response)?
        .get("planId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|plan_id| !plan_id.is_empty())
        .map(str::to_owned)
}

fn period_end(response: &BillingResponse) -> Option<DateTime<Utc>> {
    let data = subscription_data(response)?;
    let text = data.get("currentPeriodEnd").and_then(Value::as_str)?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| parse_naive_utc(text))
}

/// The endpoint reports `2026-09-01T00:00:00` without a zone suffix; treat the
/// truncated value as UTC, as the vendor's dashboard does.
fn parse_naive_utc(text: &str) -> Option<DateTime<Utc>> {
    text.get(..19)
        .and_then(|prefix| prefix.parse::<chrono::NaiveDateTime>().ok())
        .map(|value| value.and_utc())
}

fn monthly_quota(
    monthly_credits: Option<f64>,
    grant: Option<f64>,
    resets_at: Option<DateTime<Utc>>,
) -> Result<QuotaWindow, CommandCodeError> {
    let remaining = monthly_credits.ok_or(CommandCodeError::InvalidResponse)?;
    if !remaining.is_finite() {
        return Err(CommandCodeError::InvalidResponse);
    }
    let (used_percent, used, limit) = match grant.filter(|grant| *grant > 0.0) {
        Some(grant) => {
            let used = (grant - remaining).clamp(0.0, grant);
            (used / grant * 100.0, Some(used), Some(grant))
        }
        None => (0.0, None, None),
    };
    Ok(QuotaWindow {
        id: "monthly".into(),
        label: "Monthly".into(),
        used_percent: used_percent.clamp(0.0, 100.0),
        resets_at,
        period_seconds: 30 * 24 * 60 * 60,
        format: QuotaFormat::Percent,
        used_value: used,
        limit_value: limit,
        unit: grant.map(|_| "$".into()),
        estimated: grant.is_none(),
        source_note: None,
    })
}

/// Usage windows are advertised only while the account is rate limited; an
/// absent window means "no limit to report" and is skipped.
fn map_window(
    limits: Option<&Value>,
    key: &str,
    id: &str,
    label: &str,
    period_seconds: u64,
) -> Option<QuotaWindow> {
    let window = limits?.get(key)?;
    let used = number(window.get("used"))?;
    let cap = number(window.get("cap"))?;
    if cap <= 0.0 {
        return None;
    }
    let resets_at = window
        .get("resetAt")
        .and_then(reset_epoch)
        .map(|value| DateTime::from_timestamp_millis(value).unwrap_or_default());
    Some(QuotaWindow {
        id: id.into(),
        label: label.into(),
        used_percent: (used / cap * 100.0).clamp(0.0, 100.0),
        resets_at,
        period_seconds,
        format: QuotaFormat::Percent,
        used_value: Some(used),
        limit_value: Some(cap),
        unit: Some("$".into()),
        estimated: false,
        source_note: None,
    })
}

/// Numbers may arrive as JSON numbers or numeric strings.
fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|value| {
            value.as_f64().or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.trim().parse::<f64>().ok())
            })
        })
        .filter(|value| value.is_finite())
}

fn reset_epoch(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;
    use serde_json::json;

    use super::super::CommandCodeError;
    use super::{map_billing, BillingResponse};

    fn response(status: u16, body: serde_json::Value) -> BillingResponse {
        BillingResponse {
            status: StatusCode::from_u16(status).unwrap(),
            body,
        }
    }

    fn credits_body(
        monthly: &str,
        five_hour: Option<(f64, f64)>,
        weekly: Option<(f64, f64)>,
    ) -> serde_json::Value {
        let mut limits = json!({"limited": five_hour.is_some() || weekly.is_some()});
        if let Some((used, cap)) = five_hour {
            limits["fiveHour"] = json!({"used": used, "cap": cap, "resetAt": 1_800_000_000_000i64});
        }
        if let Some((used, cap)) = weekly {
            limits["weekly"] = json!({"used": used, "cap": cap});
        }
        json!({"credits": {"monthlyCredits": monthly, "purchasedCredits": 5, "freeCredits": 0}, "windowLimits": limits})
    }

    #[test]
    fn maps_monthly_and_rate_limit_windows() {
        let billing = map_billing(
            response(200, credits_body("23.5", Some((28.0, 70.0)), Some((10.0, 70.0)))),
            Some(response(
                200,
                json!({"success": true, "data": {"planId": "individual-goat", "currentPeriodEnd": "2026-10-01T00:00:00"}}),
            )),
        )
        .unwrap();

        assert_eq!(billing.plan.as_deref(), Some("GOAT"));
        let monthly = &billing.quotas[0];
        assert_eq!(monthly.id, "monthly");
        assert_eq!(monthly.used_value, Some(46.5));
        assert_eq!(monthly.limit_value, Some(70.0));
        assert!((monthly.used_percent - 66.428_571_428_571_43).abs() < 1e-9);
        assert_eq!(monthly.unit.as_deref(), Some("$"));
        assert!(!monthly.estimated);
        assert!(monthly.resets_at.is_some());

        let session = &billing.quotas[1];
        assert_eq!(session.id, "session");
        assert_eq!(session.period_seconds, 5 * 60 * 60);
        assert!((session.used_percent - 40.0).abs() < 1e-9);
        assert!(session.resets_at.is_some());

        let weekly = &billing.quotas[2];
        assert_eq!(weekly.id, "weekly");
        assert_eq!(weekly.period_seconds, 7 * 24 * 60 * 60);
        assert!((weekly.used_percent - 100.0 / 7.0).abs() < 1e-9);
    }

    #[test]
    fn monthly_without_a_known_plan_stays_estimated() {
        let billing = map_billing(
            response(200, credits_body("23.5", None, None)),
            Some(response(
                200,
                json!({"success": true, "data": {"planId": "mystery-plan"}}),
            )),
        )
        .unwrap();

        let monthly = &billing.quotas[0];
        assert_eq!(billing.plan.as_deref(), None);
        assert_eq!(monthly.used_value, None);
        assert_eq!(monthly.limit_value, None);
        assert_eq!(monthly.used_percent, 0.0);
        assert!(monthly.estimated);
        assert!(!billing.quotas.iter().any(|quota| quota.id == "session"));
    }

    #[test]
    fn a_missing_subscription_keeps_the_report_alive() {
        let billing = map_billing(
            response(200, credits_body("30", Some((35.0, 70.0)), None)),
            None,
        )
        .unwrap();

        assert_eq!(billing.plan, None);
        assert_eq!(billing.quotas.len(), 2);
        assert!(billing.quotas[0].resets_at.is_none());
    }

    #[test]
    fn a_failed_subscription_is_ignored_but_a_failed_credits_call_is_fatal() {
        let degraded = map_billing(
            response(200, credits_body("30", None, None)),
            Some(response(500, json!({"error": "boom"}))),
        );
        assert!(degraded.is_ok());

        let error = map_billing(
            response(401, json!({"message": "invalid key"})),
            Some(response(
                200,
                json!({"success": true, "data": {"planId": "individual-go"}}),
            )),
        )
        .unwrap_err();
        assert_eq!(error, CommandCodeError::InvalidKey);
    }

    #[test]
    fn missing_monthly_credits_is_an_invalid_response() {
        let error = map_billing(
            response(200, json!({"credits": {}, "windowLimits": {}})),
            None,
        )
        .unwrap_err();
        assert_eq!(error, CommandCodeError::InvalidResponse);
    }

    #[test]
    fn zero_caps_are_skipped() {
        let billing = map_billing(
            response(200, credits_body("10", Some((0.0, 0.0)), Some((1.0, 0.0)))),
            None,
        )
        .unwrap();

        assert_eq!(billing.quotas.len(), 1);
        assert_eq!(billing.quotas[0].id, "monthly");
    }
}
