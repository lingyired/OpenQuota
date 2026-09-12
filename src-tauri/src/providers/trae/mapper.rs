use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{QuotaFormat, QuotaWindow, StatusMetric, StatusTone};

#[derive(Debug, PartialEq)]
pub struct TokenInfo {
    pub token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, PartialEq)]
pub struct EntitlementMetrics {
    pub plan: Option<String>,
    pub quota: Option<QuotaWindow>,
    pub status: Option<StatusMetric>,
}

pub fn map_token(body: &Value) -> Option<TokenInfo> {
    let result = body.get("Result")?.as_object()?;
    let token = result.get("Token")?.as_str()?.trim();
    if token.is_empty() {
        return None;
    }
    let expires_at = result
        .get("ExpiredAt")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    Some(TokenInfo {
        token: token.to_owned(),
        expires_at,
    })
}

pub fn map_entitlement(body: &Value) -> Option<EntitlementMetrics> {
    let summary = body.get("usage_summary")?.as_object()?;
    let total = number(summary.get("total_amount"))?;
    let consumed = number(summary.get("consumed_amount")).unwrap_or(0.0);
    if total <= 0.0 {
        return Some(EntitlementMetrics {
            plan: None,
            quota: None,
            status: Some(StatusMetric {
                id: "status".into(),
                label: "Status".into(),
                text: "No active credits".into(),
                tone: StatusTone::Warning,
                subtitle: Some("This account has no active Trae credit plan.".into()),
            }),
        });
    }

    let ratio = number(summary.get("consumption_ratio"))
        .map(|value| value * 100.0)
        .unwrap_or_else(|| consumed / total * 100.0);
    Some(EntitlementMetrics {
        plan: Some("Credits".into()),
        quota: Some(QuotaWindow {
            id: "credits".into(),
            label: "Credits".into(),
            used_percent: ratio.clamp(0.0, 100.0),
            resets_at: None,
            period_seconds: 0,
            format: QuotaFormat::Count,
            used_value: Some(consumed.max(0.0)),
            limit_value: Some(total),
            unit: Some("credits".into()),
            estimated: false,
            source_note: None,
        }),
        status: None,
    })
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use serde_json::json;

    use super::{map_entitlement, map_token, EntitlementMetrics, TokenInfo};

    #[test]
    fn token_response_maps_token_and_optional_expiry() {
        let body = json!({
            "Result": {
                "Token": "  jwt-token  ",
                "ExpiredAt": "2026-09-25T20:49:00+08:00"
            }
        });

        let token = map_token(&body).unwrap();
        assert_eq!(token.token, "jwt-token");
        assert_eq!(
            token.expires_at,
            Some("2026-09-25T12:49:00Z".parse::<DateTime<Utc>>().unwrap())
        );
        assert!(matches!(token, TokenInfo { .. }));
    }

    #[test]
    fn malformed_token_response_is_rejected() {
        assert!(map_token(&json!({"Result": {}})).is_none());
        assert!(map_token(&json!({"Result": {"Token": "   "}})).is_none());
    }

    #[test]
    fn cn_entitlement_maps_credit_usage_summary() {
        let body = json!({
            "is_credits_billing": true,
            "usage_summary": {
                "total_amount": 3100,
                "consumed_amount": 1452.22,
                "consumption_ratio": 0.468
            }
        });

        let mapped = map_entitlement(&body).unwrap();
        let quota = mapped.quota.unwrap();
        assert_eq!(mapped.plan.as_deref(), Some("Credits"));
        assert_eq!(quota.id, "credits");
        assert_eq!(quota.label, "Credits");
        assert_eq!(quota.used_value, Some(1452.22));
        assert_eq!(quota.limit_value, Some(3100.0));
        assert!((quota.used_percent - 46.8).abs() < 0.001);
        assert_eq!(quota.unit.as_deref(), Some("credits"));
        assert!(mapped.status.is_none());
    }

    #[test]
    fn missing_ratio_is_derived_from_amounts() {
        let body = json!({
            "usage_summary": {
                "total_amount": 200,
                "consumed_amount": 50
            }
        });

        let quota = map_entitlement(&body).unwrap().quota.unwrap();
        assert_eq!(quota.used_percent, 25.0);
    }

    #[test]
    fn non_positive_total_becomes_a_notice_instead_of_a_broken_meter() {
        let body = json!({
            "usage_summary": {
                "total_amount": 0,
                "consumed_amount": 0,
                "consumption_ratio": 0
            }
        });

        assert_eq!(
            map_entitlement(&body),
            Some(EntitlementMetrics {
                plan: None,
                quota: None,
                status: Some(crate::models::StatusMetric {
                    id: "status".into(),
                    label: "Status".into(),
                    text: "No active credits".into(),
                    tone: crate::models::StatusTone::Warning,
                    subtitle: Some("This account has no active Trae credit plan.".into()),
                }),
            })
        );
    }

    #[test]
    fn missing_usage_summary_is_rejected() {
        assert!(map_entitlement(&json!({})).is_none());
        assert!(map_entitlement(&json!({"usage_summary": {"total_amount": "bad"}})).is_none());
    }
}
