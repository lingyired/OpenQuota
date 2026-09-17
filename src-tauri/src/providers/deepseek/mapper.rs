use std::str::FromStr;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use crate::models::{MetricValue, MetricValueKind, ValueMetric};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepSeekMapError {
    Authentication,
    InvalidResponse,
}

#[derive(Debug, Default, PartialEq)]
pub struct SummaryMetrics {
    pub plan: Option<String>,
    pub values: Vec<ValueMetric>,
    pub currencies: Vec<String>,
}

pub fn map_summary(body: &Value) -> Result<SummaryMetrics, DeepSeekMapError> {
    let data = business_data(body)?;
    let normal_wallets = data
        .get("normal_wallets")
        .and_then(Value::as_array)
        .ok_or(DeepSeekMapError::InvalidResponse)?;
    let bonus_wallets = data
        .get("bonus_wallets")
        .and_then(Value::as_array)
        .ok_or(DeepSeekMapError::InvalidResponse)?;
    let total_costs = data
        .get("total_costs")
        .and_then(Value::as_array)
        .ok_or(DeepSeekMapError::InvalidResponse)?;

    let mut balances = Vec::new();
    for wallet in normal_wallets.iter().chain(bonus_wallets) {
        if let Some((currency, amount)) = amount(wallet, "balance") {
            add_amount(&mut balances, currency, amount);
        }
    }

    let mut total_spends = Vec::new();
    for cost in total_costs {
        if let Some((currency, amount)) = amount(cost, "amount") {
            add_amount(&mut total_spends, currency, amount);
        }
    }

    let currencies = balances
        .iter()
        .chain(total_spends.iter())
        .map(|(currency, _)| currency.clone())
        .fold(Vec::new(), |mut currencies, currency| {
            if !currencies.contains(&currency) {
                currencies.push(currency);
            }
            currencies
        });

    let mut values = Vec::new();
    if !balances.is_empty() {
        values.push(metric("balance", "Balance", balance_values(balances)));
    }
    if !total_spends.is_empty() {
        values.push(metric(
            "totalSpend",
            "Total Spend",
            balance_values(total_spends),
        ));
    }

    Ok(SummaryMetrics {
        plan: Some("Available".into()),
        values,
        currencies,
    })
}

pub fn map_today_cost(
    body: &Value,
    currencies: &[String],
) -> Result<Vec<MetricValue>, DeepSeekMapError> {
    let data = business_data(body)?;
    let groups = data
        .get("data")
        .and_then(Value::as_array)
        .ok_or(DeepSeekMapError::InvalidResponse)?;
    let mut costs = currencies
        .iter()
        .map(|currency| (currency.clone(), Decimal::ZERO))
        .collect::<Vec<_>>();

    for group in groups {
        let Some(group) = group.as_object() else {
            continue;
        };
        let Some(currency) = currency(group) else {
            continue;
        };
        let Some((_, total)) = costs
            .iter_mut()
            .find(|(known_currency, _)| known_currency == &currency)
        else {
            continue;
        };
        let Some(series) = group.get("series").and_then(Value::as_array) else {
            continue;
        };
        for series_entry in series {
            let Some(buckets) = series_entry
                .as_object()
                .and_then(|entry| entry.get("buckets"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for bucket in buckets {
                let Some(amount) = bucket
                    .as_object()
                    .and_then(|bucket| decimal(bucket.get("cost")))
                else {
                    continue;
                };
                *total += amount;
            }
        }
    }

    Ok(costs
        .into_iter()
        .map(|(currency, amount)| metric_value(currency, amount))
        .collect())
}

fn business_data(body: &Value) -> Result<&Map<String, Value>, DeepSeekMapError> {
    let object = body.as_object().ok_or(DeepSeekMapError::InvalidResponse)?;
    check_code(object.get("code"))?;
    let data = object
        .get("data")
        .and_then(Value::as_object)
        .ok_or(DeepSeekMapError::InvalidResponse)?;
    check_code(data.get("biz_code"))?;
    data.get("biz_data")
        .and_then(Value::as_object)
        .ok_or(DeepSeekMapError::InvalidResponse)
}

fn check_code(value: Option<&Value>) -> Result<(), DeepSeekMapError> {
    match value.and_then(Value::as_i64) {
        Some(0) => Ok(()),
        Some(40002 | 40003) => Err(DeepSeekMapError::Authentication),
        _ => Err(DeepSeekMapError::InvalidResponse),
    }
}

fn amount(entry: &Value, field: &str) -> Option<(String, Decimal)> {
    let entry = entry.as_object()?;
    let currency = currency(entry)?;
    let amount = decimal(entry.get(field))?;
    Some((currency, amount))
}

fn currency(entry: &Map<String, Value>) -> Option<String> {
    let currency = entry.get("currency")?.as_str()?.trim().to_ascii_uppercase();
    (!currency.is_empty()).then_some(currency)
}

fn decimal(value: Option<&Value>) -> Option<Decimal> {
    let value = match value? {
        Value::String(value) => Decimal::from_str(value.trim()).ok()?,
        Value::Number(value) => Decimal::from_str(&value.to_string()).ok()?,
        _ => return None,
    };
    (value >= Decimal::ZERO).then_some(value)
}

fn add_amount(values: &mut Vec<(String, Decimal)>, currency: String, amount: Decimal) {
    if let Some((_, total)) = values
        .iter_mut()
        .find(|(known_currency, _)| known_currency == &currency)
    {
        *total += amount;
    } else {
        values.push((currency, amount));
    }
}

fn balance_values(values: Vec<(String, Decimal)>) -> Vec<MetricValue> {
    values
        .into_iter()
        .map(|(currency, amount)| metric_value(currency, amount))
        .collect()
}

fn metric_value(currency: String, amount: Decimal) -> MetricValue {
    MetricValue {
        number: amount.to_f64().unwrap_or(0.0),
        kind: MetricValueKind::Currency,
        label: Some(currency),
        estimated: false,
    }
}

fn metric(id: &str, label: &str, values: Vec<MetricValue>) -> ValueMetric {
    ValueMetric {
        id: id.into(),
        label: label.into(),
        values,
        expiries_at: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{map_summary, map_today_cost, DeepSeekMapError};

    #[test]
    fn summary_combines_wallets_and_maps_total_spend_by_currency() {
        let body = json!({
            "code": 0,
            "data": {
                "biz_code": 0,
                "biz_data": {
                    "normal_wallets": [
                        {"currency": "CNY", "balance": 100.0},
                        {"currency": "USD", "balance": 2.0}
                    ],
                    "bonus_wallets": [{"currency": "CNY", "balance": 10.0}],
                    "total_costs": [{"currency": "CNY", "amount": "45.50"}]
                }
            }
        });

        let mapped = map_summary(&body).unwrap();
        assert_eq!(mapped.plan.as_deref(), Some("Available"));
        assert_eq!(mapped.currencies, ["CNY", "USD"]);
        assert_eq!(mapped.values.len(), 2);
        assert_eq!(mapped.values[0].id, "balance");
        assert_eq!(mapped.values[0].label, "Balance");
        assert_eq!(mapped.values[0].values[0].number, 110.0);
        assert_eq!(mapped.values[0].values[0].label.as_deref(), Some("CNY"));
        assert_eq!(mapped.values[0].values[1].number, 2.0);
        assert_eq!(mapped.values[0].values[1].label.as_deref(), Some("USD"));
        assert_eq!(mapped.values[1].id, "totalSpend");
        assert_eq!(mapped.values[1].values[0].number, 45.5);
    }

    #[test]
    fn today_cost_sums_buckets_with_decimal_arithmetic_and_keeps_zero_currencies() {
        let body = json!({
            "code": 0,
            "data": {
                "biz_code": 0,
                "biz_data": {
                    "data": [{
                        "currency": "CNY",
                        "series": [
                            {"buckets": [{"cost": "0.10"}]},
                            {"buckets": [{"cost": "0.2003"}]}
                        ]
                    }]
                }
            }
        });

        let values = map_today_cost(&body, &["CNY".into(), "USD".into()]).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].number, 0.3003);
        assert_eq!(values[1].number, 0.0);
    }

    #[test]
    fn authentication_and_malformed_envelopes_are_distinct() {
        assert_eq!(
            map_summary(&json!({"code": 40002, "data": null})),
            Err(DeepSeekMapError::Authentication)
        );
        assert_eq!(
            map_summary(&json!({
                "code": 0,
                "data": {"biz_code": 40003, "biz_data": {}}
            })),
            Err(DeepSeekMapError::Authentication)
        );
        assert_eq!(
            map_summary(&json!({
                "code": 1,
                "data": {"biz_code": 0, "biz_data": {}}
            })),
            Err(DeepSeekMapError::InvalidResponse)
        );
    }

    #[test]
    fn missing_expected_shape_is_rejected() {
        assert_eq!(
            map_summary(&json!({
                "code": 0,
                "data": {"biz_code": 0, "biz_data": {}}
            })),
            Err(DeepSeekMapError::InvalidResponse)
        );
    }
}
