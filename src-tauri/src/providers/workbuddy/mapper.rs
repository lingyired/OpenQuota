use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkBuddyMappingError {
    #[error("WorkBuddy returned no usable package data.")]
    NoPackageData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourcePackage {
    pub package_code: String,
    pub package_name: String,
    pub total: f64,
    pub remaining: f64,
    pub used: f64,
    pub status: Option<i64>,
    pub expire_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MappedResources {
    pub packages: Vec<ResourcePackage>,
    pub total: f64,
    pub remaining: f64,
    pub used: f64,
    pub expiring_soon_remaining: f64,
    pub expired_remaining: f64,
    pub soonest_expire_at: Option<DateTime<Utc>>,
}

const TOTAL_KEYS: &[&str] = &[
    "CycleCapacitySizePrecise",
    "CycleCapacitySize",
    "CycleTotalCapacity",
    "CapacitySizePrecise",
    "CapacitySize",
    "SlicePeriodCapacitySizePrecise",
    "SlicePeriodCapacitySize",
];
const REMAINING_KEYS: &[&str] = &[
    "CycleCapacityRemainPrecise",
    "CycleCapacityRemain",
    "CycleRemainCapacity",
    "CapacityRemainPrecise",
    "CapacityRemain",
    "SlicePeriodCapacityRemainPrecise",
    "SlicePeriodCapacityRemain",
];
const USED_KEYS: &[&str] = &[
    "CycleCapacityUsedPrecise",
    "CycleCapacityUsed",
    "CycleUsedCapacity",
    "CapacityUsedPrecise",
    "CapacityUsed",
    "SlicePeriodCapacityUsedPrecise",
    "SlicePeriodCapacityUsed",
];
const EXPIRE_KEYS: &[&str] = &[
    "DeductionEndTime",
    "deductionEndTime",
    "ExpiredTime",
    "expiredTime",
    "CycleEndTime",
    "cycleEndTime",
];
const STATUS_KEYS: &[&str] = &["Status", "status"];
const NAME_KEYS: &[&str] = &["PackageName", "packageName"];
const CODE_KEYS: &[&str] = &["PackageCode", "packageCode"];

/// Extracts a package list from the response shapes observed in the WorkBuddy web client.
/// An empty array is intentionally returned as a valid result instead of being treated as a
/// missing response.
pub fn extract_resources<'a>(body: &'a Value, list_name: &str) -> Option<&'a Vec<Value>> {
    let names: &[&str] = match list_name {
        "Accounts" => &["Accounts", "accounts"],
        "Packages" => &["Packages", "packages"],
        _ => return None,
    };
    let paths: &[&[&str]] = &[
        &["data"],
        &["data", "data"],
        &["data", "Response", "Data"],
        &["data", "data", "Response", "Data"],
    ];
    paths.iter().find_map(|path| {
        let object = path.iter().try_fold(body, |value, key| value.get(*key))?;
        names.iter().find_map(|name| object.get(*name)?.as_array())
    })
}

pub fn map_resources(
    summary: Option<&Value>,
    paid: Option<&Value>,
    free: Option<&Value>,
    now: DateTime<Local>,
) -> Result<MappedResources, WorkBuddyMappingError> {
    let summary_packages = summary.and_then(|body| extract_resources(body, "Packages"));
    let paid_packages = paid.and_then(|body| extract_resources(body, "Accounts"));
    let free_packages = free.and_then(|body| extract_resources(body, "Accounts"));

    if summary_packages.is_none() && paid_packages.is_none() && free_packages.is_none() {
        return Err(WorkBuddyMappingError::NoPackageData);
    }

    let mut details = Vec::new();
    if let Some(items) = paid_packages {
        details.extend(parse_packages(items));
    }
    if let Some(items) = free_packages {
        details.extend(parse_packages(items));
    }

    let detail_codes = details
        .iter()
        .map(|package| package.package_code.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut packages = details;
    if let Some(items) = summary_packages {
        packages.extend(
            parse_packages(items)
                .into_iter()
                .filter(|package| !detail_codes.contains(&package.package_code)),
        );
    }

    // Keep the first detail record for a package code. Paid/free details are preferred over the
    // summary because they usually carry the most accurate expiration timestamp.
    let mut by_code = HashMap::new();
    for package in packages {
        by_code
            .entry(package.package_code.clone())
            .or_insert(package);
    }
    let mut packages = by_code.into_values().collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        left.expire_at
            .cmp(&right.expire_at)
            .then_with(|| left.package_code.cmp(&right.package_code))
    });

    // Treat the seventh calendar day as part of the "expiring soon" window.
    // WorkBuddy commonly returns date-only expirations, which represent the end of
    // that local calendar day rather than the exact current time plus seven days.
    let soon_limit = now.date_naive() + chrono::Days::new(7);
    let mut total = 0.0;
    let mut remaining = 0.0;
    let mut used = 0.0;
    let mut expiring_soon_remaining = 0.0;
    let mut expired_remaining = 0.0;
    let mut soonest_expire_at = None;
    for package in &packages {
        total += package.total;
        remaining += package.remaining;
        used += package.used;
        if package.remaining <= 0.0 {
            continue;
        }
        if let Some(expire_at) = package.expire_at {
            if expire_at <= now.with_timezone(&Utc) {
                expired_remaining += package.remaining;
            } else {
                soonest_expire_at = Some(
                    soonest_expire_at
                        .map_or(expire_at, |current: DateTime<Utc>| current.min(expire_at)),
                );
                if expire_at.with_timezone(&Local).date_naive() <= soon_limit {
                    expiring_soon_remaining += package.remaining;
                }
            }
        }
    }

    Ok(MappedResources {
        packages,
        total: non_negative(total),
        remaining: non_negative(remaining),
        used: non_negative(used),
        expiring_soon_remaining: non_negative(expiring_soon_remaining),
        expired_remaining: non_negative(expired_remaining),
        soonest_expire_at,
    })
}

fn parse_packages(items: &[Value]) -> Vec<ResourcePackage> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, value)| parse_package(value, index))
        .collect()
}

fn parse_package(value: &Value, index: usize) -> Option<ResourcePackage> {
    let object = value.as_object()?;
    let detail = object
        .get("SlicePeriodUsageDetails")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_object);

    let number_at = |keys: &[&str]| {
        first_number(object, keys).or_else(|| detail.and_then(|detail| first_number(detail, keys)))
    };
    let expire_at = first_value(object, EXPIRE_KEYS)
        .or_else(|| detail.and_then(|detail| first_value(detail, EXPIRE_KEYS)))
        .and_then(parse_datetime);
    let package_name = first_string(object, NAME_KEYS)
        .or_else(|| detail.and_then(|detail| first_string(detail, NAME_KEYS)))
        .unwrap_or_default();
    let package_code = first_string(object, CODE_KEYS)
        .or_else(|| detail.and_then(|detail| first_string(detail, CODE_KEYS)))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if package_name.is_empty() {
                format!("package-{index}")
            } else {
                package_name.clone()
            }
        });

    let raw_total = number_at(TOTAL_KEYS);
    let raw_remaining = number_at(REMAINING_KEYS);
    let raw_used = number_at(USED_KEYS);
    let total = raw_total
        .or_else(|| match (raw_remaining, raw_used) {
            (Some(remaining), Some(used)) => Some(remaining + used),
            _ => raw_remaining.or(raw_used),
        })
        .unwrap_or(0.0)
        .max(0.0);
    let remaining = raw_remaining
        .unwrap_or_else(|| (total - raw_used.unwrap_or(0.0)).max(0.0))
        .max(0.0);
    let used = raw_used
        .unwrap_or_else(|| (total - remaining).max(0.0))
        .max(0.0);

    Some(ResourcePackage {
        package_code,
        package_name,
        total,
        remaining,
        used,
        status: first_number(object, STATUS_KEYS)
            .or_else(|| detail.and_then(|detail| first_number(detail, STATUS_KEYS)))
            .map(|value| value as i64),
        expire_at,
    })
}

fn first_value<'a>(object: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn first_string(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    first_value(object, keys)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn first_number(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| parse_number(object.get(*key)))
        .filter(|value| value.is_finite())
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

fn parse_datetime(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(number) = parse_number(Some(value)) {
        if !number.is_finite() {
            return None;
        }
        let millis = if number.abs() < 1.0e10 {
            number * 1000.0
        } else {
            number
        };
        return DateTime::from_timestamp_millis(millis.trunc() as i64);
    }
    let text = value.as_str()?.trim();
    DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").map(|value| {
                Local
                    .from_local_datetime(&value)
                    .single()
                    .unwrap_or_else(Local::now)
                    .with_timezone(&Utc)
            })
        })
        .or_else(|_| {
            NaiveDate::parse_from_str(text, "%Y-%m-%d").map(|value| {
                Local
                    .from_local_datetime(&value.and_hms_opt(23, 59, 59).unwrap())
                    .single()
                    .unwrap_or_else(Local::now)
                    .with_timezone(&Utc)
            })
        })
        .ok()
}

fn non_negative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone, Utc};
    use serde_json::json;

    use super::{extract_resources, map_resources};

    #[test]
    fn extracts_nested_package_paths_and_keeps_empty_arrays_successful() {
        let body = json!({"data":{"data":{"Response":{"Data":{"Packages":[]}}}}});
        assert_eq!(extract_resources(&body, "Packages").unwrap().len(), 0);
    }

    #[test]
    fn maps_numbers_dates_and_detail_fallbacks() {
        let now = Local.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let body = json!({"data":{"Packages":[{
            "PackageCode":"summary",
            "PackageName":"Summary",
            "SlicePeriodUsageDetails":[{
                "CycleCapacityRemain":"75.25",
                "CycleCapacityUsedPrecise":25.25,
                "DeductionEndTime":"2026-09-18",
                "Status":"0"
            }]
        }]}});
        let mapped = map_resources(Some(&body), None, None, now).unwrap();
        assert_eq!(mapped.total, 100.5);
        assert_eq!(mapped.remaining, 75.25);
        assert_eq!(mapped.used, 25.25);
        assert_eq!(
            mapped.packages[0].expire_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 18, 15, 59, 59).unwrap())
        );
        assert_eq!(mapped.expiring_soon_remaining, 75.25);
    }

    #[test]
    fn paid_and_free_details_override_summary_by_package_code() {
        let now = Local.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let summary = json!({"data":{"Packages":[{"PackageCode":"same","CapacitySize":100,"CapacityRemain":10}]}});
        let paid = json!({"data":{"Accounts":[{"PackageCode":"same","CapacitySize":50,"CapacityRemain":40}]}});
        let free = json!({"data":{"Accounts":[]}});
        let mapped = map_resources(Some(&summary), Some(&paid), Some(&free), now).unwrap();
        assert_eq!(mapped.packages.len(), 1);
        assert_eq!(mapped.total, 50.0);
        assert_eq!(mapped.remaining, 40.0);
    }
}
