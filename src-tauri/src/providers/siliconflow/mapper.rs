use serde_json::Value;

use crate::models::{MetricValue, MetricValueKind, ValueMetric};

use super::{SiliconFlowError, Site};

/// Business code of a successful SiliconFlow response. The API answers `HTTP 200`
/// with a non-`20000` code when a request is rejected, so the transport status is
/// not enough on its own.
pub const SUCCESS_CODE: i64 = 20000;
/// Business code behind `{"message":"Token is invalid."}`.
pub const INVALID_KEY_CODE: i64 = 30014;

#[derive(Debug, Default, PartialEq)]
pub struct BalanceMetrics {
    pub balance: Option<ValueMetric>,
    pub granted: Option<ValueMetric>,
    pub recharged: Option<ValueMetric>,
}

pub fn business_code(body: &Value) -> Option<i64> {
    number(body.get("code")).map(|code| code as i64)
}

pub fn is_invalid_key(body: &Value) -> bool {
    business_code(body) == Some(INVALID_KEY_CODE)
}

/// Applies the business flags SiliconFlow returns inside a successful HTTP
/// response before reading the balance out of `data`.
pub fn map_usage(site: Site, body: &Value) -> Result<BalanceMetrics, SiliconFlowError> {
    if is_invalid_key(body) {
        return Err(SiliconFlowError::InvalidKey(site));
    }
    if business_code(body).is_some_and(|code| code != SUCCESS_CODE)
        || body.get("status").and_then(Value::as_bool) == Some(false)
    {
        return Err(SiliconFlowError::InvalidResponse);
    }
    map_balance(site, body)
}

pub fn map_balance(site: Site, body: &Value) -> Result<BalanceMetrics, SiliconFlowError> {
    let data = body
        .get("data")
        .and_then(Value::as_object)
        .ok_or(SiliconFlowError::InvalidResponse)?;
    let granted = number(data.get("balance"));
    let recharged = number(data.get("chargeBalance"));
    let total = number(data.get("totalBalance")).or(match (granted, recharged) {
        (Some(granted), Some(recharged)) => Some(granted + recharged),
        (Some(granted), None) => Some(granted),
        (None, Some(recharged)) => Some(recharged),
        (None, None) => None,
    });
    if total.is_none() && granted.is_none() && recharged.is_none() {
        return Err(SiliconFlowError::InvalidResponse);
    }
    let currency = site.currency();
    Ok(BalanceMetrics {
        balance: total.map(|value| money("balance", "Balance", currency, value)),
        granted: granted.map(|value| money("granted", "Granted Balance", currency, value)),
        recharged: recharged.map(|value| money("recharged", "Recharged Balance", currency, value)),
    })
}

fn money(id: &str, label: &str, currency: &str, number: f64) -> ValueMetric {
    ValueMetric {
        id: id.into(),
        label: label.into(),
        values: vec![MetricValue {
            number,
            kind: MetricValueKind::Currency,
            label: Some(currency.into()),
            estimated: false,
        }],
        expiries_at: Vec::new(),
    }
}

/// SiliconFlow serializes balances as decimal strings (`"totalBalance":"10.88"`),
/// but tolerates numbers too.
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

    use super::{business_code, is_invalid_key, map_usage, INVALID_KEY_CODE, SUCCESS_CODE};
    use crate::providers::siliconflow::Site;

    fn success_body() -> serde_json::Value {
        json!({
            "code": SUCCESS_CODE,
            "message": "OK",
            "status": true,
            "data": {
                "id": "user-1",
                "balance": "0.88",
                "chargeBalance": "10.00",
                "totalBalance": "10.88",
                "status": "normal"
            }
        })
    }

    #[test]
    fn maps_the_three_balances_with_the_site_currency() {
        let mapped = map_usage(Site::Global, &success_body()).unwrap();
        let read = |metric: &crate::models::ValueMetric| {
            (
                metric.id.clone(),
                metric.values[0].number,
                metric.values[0].label.clone(),
            )
        };
        assert_eq!(
            read(mapped.balance.as_ref().unwrap()),
            ("balance".into(), 10.88, Some("USD".into()))
        );
        assert_eq!(
            read(mapped.granted.as_ref().unwrap()),
            ("granted".into(), 0.88, Some("USD".into()))
        );
        assert_eq!(
            read(mapped.recharged.as_ref().unwrap()),
            ("recharged".into(), 10.0, Some("USD".into()))
        );

        let cn = map_usage(Site::Cn, &success_body()).unwrap();
        assert_eq!(cn.balance.unwrap().values[0].label, Some("CNY".into()));
    }

    #[test]
    fn derives_the_total_when_total_balance_is_missing() {
        let body = json!({
            "code": SUCCESS_CODE,
            "status": true,
            "data": {"balance": "1.25", "chargeBalance": "2.5"}
        });
        let mapped = map_usage(Site::Cn, &body).unwrap();
        assert_eq!(mapped.balance.unwrap().values[0].number, 3.75);
    }

    #[test]
    fn keeps_an_empty_account_readable() {
        let body = json!({
            "code": SUCCESS_CODE,
            "status": true,
            "data": {"balance": "0.00", "chargeBalance": "0.00", "totalBalance": "0.00"}
        });
        let mapped = map_usage(Site::Global, &body).unwrap();
        assert_eq!(mapped.balance.unwrap().values[0].number, 0.0);
    }

    #[test]
    fn invalid_keys_are_detected_inside_a_200_response() {
        let body = json!({"code": INVALID_KEY_CODE, "data": null, "message": "Token is invalid."});
        assert_eq!(business_code(&body), Some(INVALID_KEY_CODE));
        assert!(is_invalid_key(&body));
        assert!(map_usage(Site::Global, &body).is_err());
    }

    #[test]
    fn other_business_codes_and_missing_balances_are_invalid_responses() {
        let rejected = json!({"code": 30003, "status": false, "message": "rate limited"});
        assert!(map_usage(Site::Global, &rejected).is_err());

        let empty = json!({"code": SUCCESS_CODE, "status": true, "data": {}});
        assert!(map_usage(Site::Global, &empty).is_err());

        let error_only = json!({
            "code": SUCCESS_CODE,
            "status": true,
            "data": {"status": "banned"}
        });
        assert!(map_usage(Site::Global, &error_only).is_err());
    }
}
