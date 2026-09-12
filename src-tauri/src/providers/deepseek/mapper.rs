use serde_json::Value;

use crate::models::{MetricValue, MetricValueKind, ValueMetric};

#[derive(Debug, Default, PartialEq)]
pub struct BalanceMetrics {
    pub plan: Option<String>,
    pub values: Vec<ValueMetric>,
}

pub fn map_balance(body: &Value) -> Option<BalanceMetrics> {
    let object = body.as_object()?;
    let is_available = object.get("is_available")?.as_bool()?;
    let balances = object.get("balance_infos")?.as_array()?;

    let values = balances
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_object()?;
            let currency = entry.get("currency")?.as_str()?.trim();
            if currency.is_empty() {
                return None;
            }
            let amount = number(entry.get("total_balance")).filter(|amount| *amount >= 0.0)?;
            Some(MetricValue {
                number: amount,
                kind: MetricValueKind::Count,
                label: Some(currency.to_owned()),
                estimated: false,
            })
        })
        .collect::<Vec<_>>();

    let balance = (!values.is_empty()).then(|| ValueMetric {
        id: "balance".into(),
        label: "Balance".into(),
        values,
        expiries_at: Vec::new(),
    });
    Some(BalanceMetrics {
        plan: Some(if is_available {
            "Available".into()
        } else {
            "Unavailable".into()
        }),
        values: balance.into_iter().collect(),
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
    use serde_json::json;

    use super::{map_balance, BalanceMetrics};

    #[test]
    fn official_balance_maps_available_multi_currency_wallets() {
        let body = json!({
            "is_available": true,
            "balance_infos": [
                {"currency": "CNY", "total_balance": "110.00", "granted_balance": "10.00", "topped_up_balance": "100.00"},
                {"currency": "USD", "total_balance": 3.25, "granted_balance": 1.0, "topped_up_balance": 2.25}
            ]
        });

        let mapped = map_balance(&body).unwrap();
        assert_eq!(mapped.plan.as_deref(), Some("Available"));
        assert_eq!(mapped.values.len(), 1);
        assert_eq!(mapped.values[0].id, "balance");
        assert_eq!(mapped.values[0].label, "Balance");
        assert_eq!(mapped.values[0].values[0].number, 110.0);
        assert_eq!(mapped.values[0].values[0].label.as_deref(), Some("CNY"));
        assert_eq!(mapped.values[0].values[1].number, 3.25);
        assert_eq!(mapped.values[0].values[1].label.as_deref(), Some("USD"));
    }

    #[test]
    fn unavailable_balance_without_wallets_keeps_plan_only() {
        let body = json!({"is_available": false, "balance_infos": []});

        assert_eq!(
            map_balance(&body),
            Some(BalanceMetrics {
                plan: Some("Unavailable".into()),
                values: Vec::new(),
            })
        );
    }

    #[test]
    fn malformed_or_negative_balances_are_ignored() {
        let body = json!({
            "is_available": true,
            "balance_infos": [
                {"currency": "CNY", "total_balance": "-1"},
                {"currency": "", "total_balance": "2"},
                {"currency": "EUR", "total_balance": "not-a-number"},
                {"currency": "JPY", "total_balance": "8.5"}
            ]
        });

        let mapped = map_balance(&body).unwrap();
        assert_eq!(mapped.values[0].values.len(), 1);
        assert_eq!(mapped.values[0].values[0].number, 8.5);
        assert_eq!(mapped.values[0].values[0].label.as_deref(), Some("JPY"));
    }

    #[test]
    fn missing_expected_shape_is_rejected() {
        assert!(map_balance(&json!({"data": {}})).is_none());
        assert!(map_balance(&json!({"is_available": true})).is_none());
    }
}
