use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Days, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde_json::Value;

use crate::models::{
    DailyUsage, ModelUsageBreakdown, ModelUsageEntry, UsageCompleteness, UsageHistory, UsagePeriod,
    UsageUnit,
};

pub const MAX_PAGES: u32 = 100;
pub const DETAIL_LIMIT_PER_ACCOUNT: usize = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub request_id: String,
    pub credit: f64,
    pub model: String,
    pub client: Option<String>,
    pub request_time: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUsagePage {
    pub total: usize,
    pub raw_len: usize,
    pub records: Vec<UsageRecord>,
    pub invalid_rows: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageCollection {
    pub records: Vec<UsageRecord>,
    pub reported_total: usize,
    pub fetched_raw: usize,
    pub invalid_rows: usize,
    pub completeness: UsageCompleteness,
    aggregates: UsageAggregates,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct UsageAggregates {
    days: BTreeMap<NaiveDate, DayTotals>,
    global: DayTotals,
}

/// Parse one official WorkBuddy usage response. Only the seven documented fields are projected
/// into `UsageRecord`; prompt-bearing fields from the upstream row never leave this function.
pub fn parse_page(
    body: &Value,
    range_start: DateTime<Local>,
    range_end: DateTime<Local>,
) -> Option<ParsedUsagePage> {
    let data = body.get("data").unwrap_or(body);
    let rows = ["data", "records", "list", "items"]
        .iter()
        .find_map(|key| data.get(*key).and_then(Value::as_array))
        .or_else(|| data.as_array())?;
    let total = data
        .get("total")
        .and_then(parse_usize)
        .unwrap_or(rows.len());

    let mut records = Vec::with_capacity(rows.len());
    let mut invalid_rows = 0;
    for row in rows {
        let Some(object) = row.as_object() else {
            invalid_rows += 1;
            continue;
        };
        let Some(credit) = first_number(object, &["credit", "Credit"]) else {
            invalid_rows += 1;
            continue;
        };
        if !credit.is_finite() || credit < 0.0 {
            invalid_rows += 1;
            continue;
        }
        let Some(request_time) = object
            .get("requestTime")
            .or_else(|| object.get("request_time"))
            .and_then(parse_datetime)
        else {
            invalid_rows += 1;
            continue;
        };
        if request_time < range_start || request_time > range_end {
            invalid_rows += 1;
            continue;
        }

        records.push(UsageRecord {
            account_id: first_string(object, &["accountId", "account_id"]),
            account_name: first_string(object, &["accountName", "account_name"]),
            request_id: first_string(object, &["requestId", "request_id"]).unwrap_or_default(),
            credit,
            model: first_string(object, &["model", "Model"])
                .unwrap_or_else(|| "Unattributed".to_owned()),
            client: first_string(object, &["client", "Client"]),
            request_time,
        });
    }

    Some(ParsedUsagePage {
        total,
        raw_len: rows.len(),
        records,
        invalid_rows,
    })
}

pub fn collect_pages(
    pages: &[ParsedUsagePage],
    complete: bool,
    reported_total: usize,
) -> UsageCollection {
    let fetched_raw = pages.iter().map(|page| page.raw_len).sum();
    let invalid_rows = pages.iter().map(|page| page.invalid_rows).sum();
    let mut records = Vec::new();
    let mut seen = HashSet::new();
    let mut diagnostic_counts = HashMap::<Option<String>, usize>::new();
    let mut aggregates = UsageAggregates::default();
    for page in pages {
        for record in &page.records {
            let key = (
                record.request_id.clone(),
                record.request_time.timestamp_millis(),
            );
            if seen.insert(key) {
                aggregates
                    .days
                    .entry(record.request_time.date_naive())
                    .or_default()
                    .add(record);
                aggregates.global.add(record);

                let count = diagnostic_counts
                    .entry(record.account_id.clone())
                    .or_default();
                if *count < DETAIL_LIMIT_PER_ACCOUNT {
                    records.push(record.clone());
                    *count += 1;
                }
            }
        }
    }
    let completeness = if complete && invalid_rows == 0 {
        UsageCompleteness::Complete
    } else if aggregates.global.requests > 0 {
        UsageCompleteness::Partial
    } else {
        UsageCompleteness::Unavailable
    };
    UsageCollection {
        records,
        reported_total,
        fetched_raw,
        invalid_rows,
        completeness,
        aggregates,
    }
}

pub fn build_history(
    collection: &UsageCollection,
    now: DateTime<Local>,
    source_note: &str,
) -> UsageHistory {
    let today = now.date_naive();
    let range_start = today.checked_sub_days(Days::new(30)).unwrap_or(today);
    let completeness = collection.completeness;
    let mut days = BTreeMap::<NaiveDate, DayTotals>::new();
    for offset in 0..=30 {
        let Some(date) = range_start.checked_add_days(Days::new(offset)) else {
            continue;
        };
        days.insert(date, DayTotals::default());
    }

    for (date, totals) in &collection.aggregates.days {
        days.insert(*date, totals.clone());
    }
    let global = collection.aggregates.global.clone();

    let daily = days
        .iter()
        .map(|(date, totals)| daily_usage(*date, totals, completeness))
        .collect::<Vec<_>>();
    let today_period = Some(period_for_day(
        days.get(&today).unwrap_or(&DayTotals::default()),
        source_note,
        completeness,
    ));
    let yesterday_period = today.checked_sub_days(Days::new(1)).map(|date| {
        period_for_day(
            days.get(&date).unwrap_or(&DayTotals::default()),
            source_note,
            completeness,
        )
    });
    let last_30_days = Some(period_for_totals(&global, source_note, completeness));

    UsageHistory {
        today: today_period,
        yesterday: yesterday_period,
        last_30_days,
        daily,
        unknown_models: Vec::new(),
        completeness,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct DayTotals {
    credits: f64,
    requests: usize,
    models: BTreeMap<String, ModelTotals>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct ModelTotals {
    credits: f64,
    requests: usize,
}

impl DayTotals {
    fn add(&mut self, record: &UsageRecord) {
        self.credits += record.credit;
        self.requests += 1;
        let model = self.models.entry(record.model.clone()).or_default();
        model.credits += record.credit;
        model.requests += 1;
    }
}

fn daily_usage(date: NaiveDate, totals: &DayTotals, completeness: UsageCompleteness) -> DailyUsage {
    DailyUsage {
        date: date.to_string(),
        tokens: 0,
        amount: Some(round_amount(totals.credits)),
        unit: UsageUnit::Credits,
        estimated_cost_usd: None,
        estimate_complete: matches!(completeness, UsageCompleteness::Complete),
    }
}

fn period_for_day(
    totals: &DayTotals,
    source_note: &str,
    completeness: UsageCompleteness,
) -> UsagePeriod {
    period_for_totals(totals, source_note, completeness)
}

fn period_for_totals(
    totals: &DayTotals,
    source_note: &str,
    completeness: UsageCompleteness,
) -> UsagePeriod {
    UsagePeriod {
        tokens: 0,
        amount: Some(round_amount(totals.credits)),
        unit: UsageUnit::Credits,
        estimated_cost_usd: None,
        cost_estimated: false,
        estimate_complete: matches!(completeness, UsageCompleteness::Complete),
        model_breakdown: model_breakdown(totals, source_note),
        unknown_models: Vec::new(),
    }
}

fn model_breakdown(totals: &DayTotals, source_note: &str) -> Option<ModelUsageBreakdown> {
    if totals.models.is_empty() {
        return None;
    }
    let mut models = totals
        .models
        .iter()
        .map(|(model, totals)| ModelUsageEntry {
            model: model.clone(),
            total_tokens: 0,
            amount: Some(round_amount(totals.credits)),
            cost_usd: None,
            variants: None,
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        let left_totals = totals.models.get(&left.model).expect("model exists");
        let right_totals = totals.models.get(&right.model).expect("model exists");
        right_totals
            .credits
            .total_cmp(&left_totals.credits)
            .then_with(|| right_totals.requests.cmp(&left_totals.requests))
            .then_with(|| left.model.cmp(&right.model))
    });
    Some(ModelUsageBreakdown {
        models,
        source_note: source_note.to_owned(),
        unit: UsageUnit::Credits,
    })
}

fn first_string(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn first_number(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| parse_number(object.get(*key)))
}

fn parse_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_i64().map(|value| value as f64))
            .or_else(|| value.as_u64().map(|value| value as f64))
            .or_else(|| value.as_str()?.trim().parse().ok())
    })
}

fn parse_usize(value: &Value) -> Option<usize> {
    parse_number(Some(value))
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as usize)
}

fn parse_datetime(value: &Value) -> Option<DateTime<Local>> {
    if let Some(number) = parse_number(Some(value)) {
        if !number.is_finite() {
            return None;
        }
        let millis = if number.abs() < 1.0e10 {
            number * 1000.0
        } else {
            number
        };
        return DateTime::from_timestamp_millis(millis.trunc() as i64)
            .map(|value| value.with_timezone(&Local));
    }
    let text = value.as_str()?.trim();
    if let Ok(value) = DateTime::parse_from_rfc3339(text) {
        return Some(value.with_timezone(&Local));
    }
    if let Ok(value) = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S") {
        return Local.from_local_datetime(&value).single();
    }
    if let Ok(value) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return value
            .and_hms_opt(23, 59, 59)
            .and_then(|value| Local.from_local_datetime(&value).single());
    }
    None
}

fn round_amount(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};
    use serde_json::json;

    use super::{build_history, collect_pages, parse_page};
    use crate::models::{UsageCompleteness, UsageUnit};

    fn range() -> (chrono::DateTime<Local>, chrono::DateTime<Local>) {
        (
            Local.with_ymd_and_hms(2026, 8, 12, 0, 0, 0).unwrap(),
            Local.with_ymd_and_hms(2026, 9, 11, 23, 59, 59).unwrap(),
        )
    }

    #[test]
    fn projects_only_safe_fields_and_rejects_invalid_rows() {
        let (start, end) = range();
        let body = json!({
            "code": 0,
            "data": {"total": 3, "data": [
                {"requestId":"r1", "credit":1.25, "model":"m1", "requestTime":"2026-09-11 10:00:00", "input":"secret prompt"},
                {"requestId":"bad", "credit":-1, "requestTime":"2026-09-11 10:00:00"},
                {"requestId":"out", "credit":2, "requestTime":"2026-08-01 10:00:00"}
            ]}
        });
        let page = parse_page(&body, start, end).unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.invalid_rows, 2);
        assert!(!format!("{:?}", page.records).contains("secret prompt"));
    }

    #[test]
    fn deduplicates_and_marks_partial_when_rows_are_invalid() {
        let (start, end) = range();
        let body = json!({"data":{"total":2,"data":[
            {"requestId":"r1","credit":"1.5","model":"z","requestTime":"2026-09-10 10:00:00"},
            {"requestId":"r1","credit":1.5,"model":"z","requestTime":"2026-09-10 10:00:00"},
            {"requestId":"bad","credit":"NaN","requestTime":"2026-09-10 10:00:00"}
        ]}});
        let page = parse_page(&body, start, end).unwrap();
        let collection = collect_pages(&[page], true, 2);
        assert_eq!(collection.records.len(), 1);
        assert_eq!(collection.completeness, UsageCompleteness::Partial);
    }

    #[test]
    fn caps_diagnostic_records_per_account_without_changing_aggregates() {
        let (start, end) = range();
        let rows = (0..101)
            .map(|index| {
                json!({
                    "requestId": format!("r{index}"),
                    "accountId": "account-a",
                    "credit": 1,
                    "model": "m",
                    "requestTime": "2026-09-11 10:00:00"
                })
            })
            .collect::<Vec<_>>();
        let body = json!({"data":{"total":101,"data":rows}});
        let page = parse_page(&body, start, end).unwrap();
        let collection = collect_pages(&[page], true, 101);

        assert_eq!(collection.records.len(), 100);
        let history = build_history(
            &collection,
            Local.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap(),
            "WorkBuddy official usage",
        );
        assert_eq!(history.today.as_ref().unwrap().amount, Some(101.0));
    }

    #[test]
    fn builds_zero_filled_credit_history_and_sorts_models() {
        let (start, end) = range();
        let body = json!({"data":{"total":3,"data":[
            {"requestId":"1","credit":1,"model":"z","requestTime":"2026-09-11 10:00:00"},
            {"requestId":"2","credit":2,"model":"a","requestTime":"2026-09-11 11:00:00"},
            {"requestId":"3","credit":2,"model":"a","requestTime":"2026-09-11 12:00:00"}
        ]}});
        let page = parse_page(&body, start, end).unwrap();
        let collection = collect_pages(&[page], true, 3);
        let history = build_history(
            &collection,
            Local.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap(),
            "WorkBuddy official usage",
        );
        assert_eq!(history.completeness, UsageCompleteness::Complete);
        assert_eq!(history.daily.len(), 31);
        assert_eq!(history.today.as_ref().unwrap().amount, Some(5.0));
        assert_eq!(history.today.as_ref().unwrap().unit, UsageUnit::Credits);
        let models = &history
            .today
            .as_ref()
            .unwrap()
            .model_breakdown
            .as_ref()
            .unwrap()
            .models;
        assert_eq!(models[0].model, "a");
        assert_eq!(models[0].amount, Some(4.0));
    }
}
