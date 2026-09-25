use serde_json::{Map, Value};

use crate::models::{QuotaFormat, QuotaWindow};

use super::InfiniError;

/// Business code of a rejected Coding Plan key — `HTTP 401` with
/// `{"code":10021,"msg":"CodingPlan的api key不正确，请确认后再重试"}`. A GenStudio key
/// (`sk-…`) is reported the same way because the two products do not share keys.
pub const INVALID_KEY_CODE: i64 = 10021;

/// The three independent request buckets of a Coding Plan, in console order. The
/// wire keys are the vendor's own; quotas are counted in requests.
struct WindowSpec {
    key: &'static str,
    source_id: &'static str,
    label: &'static str,
    period_seconds: u64,
}

const WINDOWS: [WindowSpec; 3] = [
    WindowSpec {
        key: "5_hour",
        source_id: "fiveHour",
        label: "Session (5h)",
        period_seconds: 5 * 60 * 60,
    },
    WindowSpec {
        key: "7_day",
        source_id: "sevenDay",
        label: "Weekly",
        period_seconds: 7 * 24 * 60 * 60,
    },
    WindowSpec {
        key: "30_day",
        source_id: "thirtyDay",
        label: "Monthly",
        period_seconds: 30 * 24 * 60 * 60,
    },
];

pub fn is_invalid_key(body: &Value) -> bool {
    number(body.get("code")).map(|code| code as i64) == Some(INVALID_KEY_CODE)
}

/// Maps `{"5_hour":{"quota","used","remain"}, …}` into quota windows. The response
/// carries no reset timestamps, so windows advertise their length only.
pub fn map_usage(body: &Value) -> Result<Vec<QuotaWindow>, InfiniError> {
    let root = body.as_object().ok_or(InfiniError::InvalidResponse)?;
    let quotas = WINDOWS
        .iter()
        .filter_map(|spec| map_window(root, spec))
        .collect::<Vec<_>>();
    if quotas.is_empty() {
        return Err(InfiniError::InvalidResponse);
    }
    Ok(quotas)
}

fn map_window(root: &Map<String, Value>, spec: &WindowSpec) -> Option<QuotaWindow> {
    let bucket = root.get(spec.key)?.as_object()?;
    let quota = number(bucket.get("quota")).filter(|quota| *quota > 0.0)?;
    let used = number(bucket.get("used"))
        .or_else(|| number(bucket.get("remain")).map(|remain| quota - remain))?;
    let used = used.clamp(0.0, quota);
    Some(QuotaWindow {
        id: spec.source_id.into(),
        label: spec.label.into(),
        used_percent: (used / quota * 100.0).clamp(0.0, 100.0),
        resets_at: None,
        period_seconds: spec.period_seconds,
        format: QuotaFormat::Count,
        used_value: Some(used),
        limit_value: Some(quota),
        unit: Some("requests".into()),
        estimated: false,
        source_note: None,
    })
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{is_invalid_key, map_usage};

    const PRO_BODY: &str = r#"{
        "5_hour":  {"quota": 5000,  "used": 1250, "remain": 3750},
        "7_day":   {"quota": 30000, "used": 9000, "remain": 21000},
        "30_day":  {"quota": 60000, "used": 0,    "remain": 60000}
    }"#;

    #[test]
    fn maps_the_three_request_buckets() {
        let body: serde_json::Value = serde_json::from_str(PRO_BODY).unwrap();
        let quotas = map_usage(&body).unwrap();

        assert_eq!(
            quotas
                .iter()
                .map(|quota| (
                    quota.id.as_str(),
                    quota.used_value.unwrap(),
                    quota.limit_value.unwrap(),
                    quota.used_percent,
                ))
                .collect::<Vec<_>>(),
            [
                ("fiveHour", 1250.0, 5000.0, 25.0),
                ("sevenDay", 9000.0, 30000.0, 30.0),
                ("thirtyDay", 0.0, 60000.0, 0.0),
            ]
        );
        assert_eq!(quotas[0].label, "Session (5h)");
        assert_eq!(quotas[1].label, "Weekly");
        assert_eq!(quotas[2].label, "Monthly");
        assert_eq!(quotas[0].period_seconds, 5 * 60 * 60);
        assert_eq!(quotas[0].unit.as_deref(), Some("requests"));
        assert!(quotas.iter().all(|quota| quota.resets_at.is_none()));
    }

    #[test]
    fn derives_usage_from_the_remaining_count() {
        let body = json!({"5_hour": {"quota": 1000, "remain": 250}});
        let quotas = map_usage(&body).unwrap();
        assert_eq!(quotas[0].used_value, Some(750.0));
        assert_eq!(quotas[0].used_percent, 75.0);
    }

    #[test]
    fn buckets_without_a_quota_are_skipped_and_an_empty_body_is_an_error() {
        let partial = json!({
            "5_hour": {"quota": 0, "used": 0, "remain": 0},
            "7_day": {"quota": 6000, "used": 1500, "remain": 4500}
        });
        let quotas = map_usage(&partial).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].id, "sevenDay");

        assert!(map_usage(&json!({"error": "no plan"})).is_err());
    }

    #[test]
    fn usage_is_clamped_into_the_window() {
        let body = json!({"5_hour": {"quota": 1000, "used": 1200, "remain": 0}});
        let quotas = map_usage(&body).unwrap();
        assert_eq!(quotas[0].used_value, Some(1000.0));
        assert_eq!(quotas[0].used_percent, 100.0);
    }

    #[test]
    fn a_rejected_key_is_recognized_from_the_error_code() {
        let body = json!({"code": 10021, "msg": "CodingPlan的api key不正确，请确认后再重试"});
        assert!(is_invalid_key(&body));
        assert!(!is_invalid_key(&json!({"code": 0})));
    }
}
