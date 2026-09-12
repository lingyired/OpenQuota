use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{CreditPackage, QuotaFormat, QuotaWindow, StatusMetric, StatusTone};

#[derive(Debug, PartialEq)]
pub struct TokenInfo {
    pub token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct MappedPackage {
    credit: CreditPackage,
    work_exclusive: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct EntitlementMetrics {
    pub plan: Option<String>,
    pub quota: Option<QuotaWindow>,
    pub status: Option<StatusMetric>,
    pub packages: Vec<CreditPackage>,
    pub work_remaining: f64,
    pub general_remaining: f64,
    pub nearest_expiring: Option<CreditPackage>,
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

pub fn map_entitlement(body: &Value, now: DateTime<Utc>) -> Option<EntitlementMetrics> {
    let summary = body.get("usage_summary")?.as_object()?;
    let total = number(summary.get("total_amount"))?;
    let consumed = number(summary.get("consumed_amount")).unwrap_or(0.0);
    let mapped_packages = map_packages(body.get("user_entitlement_pack_list"), now);
    let work_remaining = mapped_packages
        .iter()
        .filter(|package| package.work_exclusive && !package.credit.unlimited)
        .map(|package| package.credit.remaining)
        .sum::<f64>()
        .max(0.0);
    let available = (total - consumed).max(0.0);
    let general_remaining = (available - work_remaining).max(0.0);
    let nearest_expiring = mapped_packages
        .iter()
        .find(|package| !package.credit.unlimited && package.credit.remaining > 0.0)
        .map(|package| package.credit.clone());
    let packages = mapped_packages
        .into_iter()
        .map(|package| package.credit)
        .collect();

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
            packages,
            work_remaining,
            general_remaining,
            nearest_expiring,
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
        packages,
        work_remaining,
        general_remaining,
        nearest_expiring,
    })
}

fn map_packages(value: Option<&Value>, now: DateTime<Utc>) -> Vec<MappedPackage> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut packages = items
        .iter()
        .enumerate()
        .filter_map(|(index, value)| map_package(value, index, now))
        .filter(|package| package.credit.unlimited || package.credit.remaining > 0.0)
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        left.credit
            .unlimited
            .cmp(&right.credit.unlimited)
            .then_with(|| left.credit.expires_at.cmp(&right.credit.expires_at))
            .then_with(|| {
                right
                    .credit
                    .remaining
                    .partial_cmp(&left.credit.remaining)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.credit.code.cmp(&right.credit.code))
    });
    packages
}

fn map_package(value: &Value, index: usize, now: DateTime<Utc>) -> Option<MappedPackage> {
    let object = value.as_object()?;
    let base = object.get("entitlement_base_info")?.as_object()?;
    let quota = base.get("quota").and_then(Value::as_object);
    let credits_limit = quota.and_then(|quota| quota.get("credits_limit"));
    let unlimited = credits_limit.is_none_or(Value::is_null)
        || number(credits_limit).is_some_and(|limit| limit <= 0.0);
    let total = if unlimited {
        0.0
    } else {
        number(credits_limit)?.max(0.0)
    };
    let used = object
        .get("usage")
        .and_then(Value::as_object)
        .and_then(|usage| number(usage.get("credits_amount")))
        .unwrap_or(0.0)
        .max(0.0);
    let remaining = if unlimited {
        0.0
    } else {
        (total - used).max(0.0)
    };
    let expires_at = number(object.get("expire_time"))
        .and_then(|value| DateTime::from_timestamp(value.trunc() as i64, 0));
    if expires_at.is_some_and(|expires_at| expires_at <= now) {
        return None;
    }

    let work_exclusive =
        number(base.get("available_endpoint")).is_some_and(|endpoint| endpoint as i64 == 1);
    let base_name = object
        .get("display_desc")
        .and_then(Value::as_str)
        .or_else(|| object.get("group_name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Trae credits");
    let product_id = number(base.get("product_id")).map(|value| value as i64);
    let code = format!(
        "trae-cn:{}:{}:{}",
        product_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "pack".into()),
        expires_at.map_or(0, |value| value.timestamp()),
        index
    );
    let category = if work_exclusive {
        "Work 专属"
    } else {
        "通用"
    };
    Some(MappedPackage {
        credit: CreditPackage {
            code,
            name: format!("{base_name} · {category}"),
            total,
            remaining,
            used,
            expires_at,
            unlimited,
        },
        work_exclusive,
    })
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_i64().map(|value| value as f64))
                .or_else(|| value.as_u64().map(|value| value as f64))
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

        let mapped = map_entitlement(&body, Utc::now()).unwrap();
        let quota = mapped.quota.unwrap();
        assert_eq!(mapped.plan.as_deref(), Some("Credits"));
        assert_eq!(quota.id, "credits");
        assert_eq!(quota.label, "Credits");
        assert_eq!(quota.used_value, Some(1452.22));
        assert_eq!(quota.limit_value, Some(3100.0));
        assert!((quota.used_percent - 46.8).abs() < 0.001);
        assert_eq!(quota.unit.as_deref(), Some("credits"));
        assert!(mapped.status.is_none());
        assert!(mapped.packages.is_empty());
        assert_eq!(mapped.general_remaining, 1647.78);
    }

    #[test]
    fn missing_ratio_is_derived_from_amounts() {
        let body = json!({
            "usage_summary": {
                "total_amount": 200,
                "consumed_amount": 50
            }
        });

        let quota = map_entitlement(&body, Utc::now()).unwrap().quota.unwrap();
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
            map_entitlement(&body, Utc::now()),
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
                packages: Vec::new(),
                work_remaining: 0.0,
                general_remaining: 0.0,
                nearest_expiring: None,
            })
        );
    }

    #[test]
    fn cn_pack_list_separates_work_and_general_credits() {
        let now = "2026-09-12T15:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let body = json!({
            "usage_summary": {
                "total_amount": 3250,
                "consumed_amount": 1452.22,
                "consumption_ratio": 0.446
            },
            "user_entitlement_pack_list": [
                {
                    "display_desc": "签到奖励",
                    "group_name": "每日签到",
                    "expire_time": 1791343210,
                    "entitlement_base_info": {
                        "product_id": 209,
                        "available_endpoint": 1,
                        "quota": {"credits_limit": 200}
                    },
                    "usage": {"credits_amount": 0}
                },
                {
                    "display_desc": "每月登录赠送",
                    "group_name": "每月登录积分",
                    "expire_time": 1790812799,
                    "entitlement_base_info": {
                        "product_id": 208,
                        "available_endpoint": 0,
                        "quota": {"credits_limit": 500}
                    },
                    "usage": {"credits_amount": 52.22}
                },
                {
                    "display_desc": "已过期",
                    "expire_time": 1780000000,
                    "entitlement_base_info": {
                        "available_endpoint": 0,
                        "quota": {"credits_limit": 100}
                    },
                    "usage": {"credits_amount": 0}
                },
                {
                    "display_desc": "免费",
                    "expire_time": 1790812799,
                    "entitlement_base_info": {
                        "available_endpoint": 0,
                        "quota": {"credits_limit": null}
                    },
                    "usage": {"credits_amount": 0}
                }
            ]
        });

        let mapped = map_entitlement(&body, now).unwrap();
        assert_eq!(mapped.work_remaining, 200.0);
        assert_eq!(mapped.general_remaining, 1597.78);
        assert_eq!(mapped.packages.len(), 3);
        assert_eq!(mapped.packages[0].name, "每月登录赠送 · 通用");
        assert_eq!(mapped.packages[0].remaining, 447.78);
        assert_eq!(mapped.packages[1].name, "签到奖励 · Work 专属");
        assert_eq!(mapped.packages[1].remaining, 200.0);
        assert!(mapped.packages[2].unlimited);
        assert_eq!(mapped.nearest_expiring.unwrap().remaining, 447.78);
    }

    #[test]
    fn missing_usage_summary_is_rejected() {
        assert!(map_entitlement(&json!({}), Utc::now()).is_none());
        assert!(map_entitlement(
            &json!({"usage_summary": {"total_amount": "bad"}}),
            Utc::now()
        )
        .is_none());
    }
}
