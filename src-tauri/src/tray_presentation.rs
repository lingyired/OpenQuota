#[cfg(any(not(target_os = "macos"), test))]
use tauri::image::Image;
use tauri::AppHandle;

#[cfg(any(target_os = "macos", target_os = "windows", test))]
use crate::models::ProviderLayout;
#[cfg(not(target_os = "macos"))]
use crate::tray_icon;
use crate::{
    models::{
        AppSettings, MetricDefinition, MetricSource, MetricValue, MetricValueKind,
        ProviderSnapshot, QuotaFormat, UsageDisplay, UsagePeriod, UsagePeriodSelection,
    },
    providers::ProviderRegistry,
    service::UsageViewState,
};

#[cfg(not(target_os = "macos"))]
const TRAY_ID: &str = "quota01-tray";

#[derive(Debug, Clone, PartialEq)]
struct TrayMetric {
    value: String,
    detail: String,
    gauge: Option<TrayGauge>,
}

/// 供 Windows taskband / macOS menubar 使用的指标解析结果：
/// 指标 id、tray 短标签与短值。
#[cfg(any(target_os = "macos", target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedTrayMetric {
    pub id: String,
    pub short_label: String,
    pub value: String,
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeInstancePlatform {
    MacOS,
    Windows,
}

/// Providers that should have a native presentation instance. This derives
/// solely from enabled settings and pinned metric definitions, so missing
/// snapshots never remove an entry point.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub(crate) fn requested_provider_entries(
    settings: &AppSettings,
    registry: &ProviderRegistry,
    platform: NativeInstancePlatform,
) -> Vec<String> {
    if platform == NativeInstancePlatform::Windows && !settings.taskband.enabled {
        return Vec::new();
    }
    settings
        .providers
        .iter()
        .filter(|provider| provider.enabled && registry.definition(&provider.id).is_some())
        .filter(|provider| {
            settings
                .taskband_providers
                .get(&provider.id)
                .is_none_or(|layout| layout.enabled)
        })
        .filter(|provider| {
            provider.metrics.iter().any(|metric| {
                metric.pinned
                    && registry
                        .metric(&metric.id)
                        .is_some_and(|definition| definition.tray.is_some())
            })
        })
        .map(|provider| provider.id.clone())
        .collect()
}

#[cfg(any(not(target_os = "macos"), test))]
#[derive(Debug, Clone, PartialEq)]
#[cfg(any(not(target_os = "macos"), test))]
struct TrayGroup {
    provider_id: String,
    metrics: Vec<TrayMetric>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TrayGauge {
    display_fraction: f64,
    #[cfg(any(not(target_os = "macos"), test))]
    remaining_fraction: f64,
}

pub fn update(
    app: &AppHandle,
    state: &UsageViewState,
    settings: &AppSettings,
    registry: &ProviderRegistry,
) {
    #[cfg(target_os = "windows")]
    crate::taskband::update(app, state, settings, registry);
    #[cfg(target_os = "macos")]
    crate::menubar::update(app, state, settings, registry);

    #[cfg(not(target_os = "macos"))]
    {
        let Some(tray) = app.tray_by_id(TRAY_ID) else {
            #[cfg(target_os = "linux")]
            if let Some(service) =
                app.try_state::<std::sync::Arc<crate::service::ProviderService>>()
            {
                service.set_native_instance_ids(Vec::new());
            }
            return;
        };
        let groups = resolved_groups(state, settings, registry);
        #[cfg(target_os = "linux")]
        if let Some(service) = app.try_state::<std::sync::Arc<crate::service::ProviderService>>() {
            service.set_native_instance_ids(groups.iter().map(|group| group.provider_id.clone()));
        }
        let tooltip = if groups.is_empty() {
            "Quota01".to_owned()
        } else {
            format!(
                "Quota01\n{}",
                groups
                    .iter()
                    .flat_map(|group| group.metrics.iter())
                    .map(|metric| metric.detail.as_str())
                    .collect::<Vec<_>>()
                    .join(" · ")
            )
        };
        #[cfg(not(target_os = "linux"))]
        if tray.set_tooltip(Some(tooltip)).is_err() {
            crate::app_warn!("tray", "tray tooltip update failed");
        }
        #[cfg(target_os = "linux")]
        let _ = tooltip;

        let icon = primary_gauge(&groups)
            .map(|gauge| tray_icon::render_gauge(gauge.display_fraction, gauge.remaining_fraction))
            .unwrap_or_else(mark_icon);
        if tray.set_icon(Some(icon)).is_err() {
            crate::app_warn!("tray", "tray icon update failed");
        }
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn primary_gauge(groups: &[TrayGroup]) -> Option<TrayGauge> {
    groups
        .iter()
        .flat_map(|group| group.metrics.iter())
        .find_map(|metric| metric.gauge)
}

#[cfg(any(not(target_os = "macos"), test))]
fn resolved_groups(
    state: &UsageViewState,
    settings: &AppSettings,
    registry: &ProviderRegistry,
) -> Vec<TrayGroup> {
    settings
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .filter_map(|provider| {
            let definition = registry.definition(&provider.id)?;
            let snapshot = state
                .providers
                .get(&provider.id)
                .and_then(|state| state.snapshot.as_ref())?;
            let metrics = provider
                .metrics
                .iter()
                .filter(|metric| metric.pinned)
                .filter_map(|metric| {
                    let metric_definition = registry.metric(&metric.id)?;
                    metric_definition.tray.as_ref()?;
                    let mut resolved =
                        tray_metric(metric_definition, snapshot, settings.usage_display)
                            .unwrap_or_else(|| tray_metric_unavailable(metric_definition));
                    resolved.detail = format!(
                        "{} {}",
                        settings.provider_display_name(definition),
                        resolved.detail
                    );
                    Some(resolved)
                })
                .collect::<Vec<_>>();
            (!metrics.is_empty()).then_some(TrayGroup {
                provider_id: definition.id.clone(),
                metrics,
            })
        })
        .collect()
}

/// 解析某个 provider 固定的（pinned，且带 tray 定义）指标为短值，供
/// Windows taskband 与 macOS menubar 取用。无快照或无固定指标时返回空列表。
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub(crate) fn pinned_provider_metrics(
    state: &UsageViewState,
    provider: &ProviderLayout,
    settings: &AppSettings,
    registry: &ProviderRegistry,
) -> Vec<ResolvedTrayMetric> {
    let snapshot = state
        .providers
        .get(&provider.id)
        .and_then(|state| state.snapshot.as_ref());
    provider
        .metrics
        .iter()
        .filter(|metric| metric.pinned)
        .filter_map(|metric| {
            let definition = registry.metric(&metric.id)?;
            let tray = definition.tray.as_ref()?;
            let value = match snapshot {
                Some(snapshot) => tray_metric(definition, snapshot, settings.usage_display)
                    .map(|resolved| resolved.value)
                    .unwrap_or_else(|| "NA".to_owned()),
                None => "--".to_owned(),
            };
            Some(ResolvedTrayMetric {
                id: metric.id.clone(),
                short_label: tray.short_label.clone(),
                value,
            })
        })
        .collect()
}

/// 某个 pinned 指标在快照中暂时没有数据时的占位显示：值显示 NA，
/// 让 menubar / taskband 仍保留用户固定出的行位（例如第二行 NA）。
#[cfg(any(not(target_os = "macos"), test))]
fn tray_metric_unavailable(definition: &MetricDefinition) -> TrayMetric {
    TrayMetric {
        value: "NA".to_owned(),
        detail: format!("{} NA", definition.label),
        gauge: None,
    }
}

fn tray_metric(
    definition: &MetricDefinition,
    snapshot: &ProviderSnapshot,
    display: UsageDisplay,
) -> Option<TrayMetric> {
    let tray = definition.tray.as_ref()?;
    let quota = |id: &str| {
        snapshot
            .quotas
            .iter()
            .find(|quota| quota.id == id)
            .map(|quota| {
                if quota.format == QuotaFormat::Count {
                    if let (Some(used), Some(limit)) = (quota.used_value, quota.limit_value) {
                        let used_fraction = (limit > 0.0).then(|| (used / limit).clamp(0.0, 1.0));
                        let value = match display {
                            UsageDisplay::Used => used,
                            UsageDisplay::Left => (limit - used).max(0.0),
                        };
                        let word = match display {
                            UsageDisplay::Used => "used",
                            UsageDisplay::Left => "left",
                        };
                        // A credit window reads as `Credits ✦3315 left`: the marker
                        // already names the unit, so the raw unit word would only
                        // repeat it. Other count windows (requests, searches) keep
                        // their unit because they have no marker of their own.
                        let credits = is_credit_unit(quota.unit.as_deref());
                        let reading = if credits {
                            format!("{CREDIT_SYMBOL}{value:.0}")
                        } else {
                            format!("{value:.0}")
                        };
                        let detail = if credits {
                            format!("{} {reading} {word}", quota.label)
                        } else {
                            format!(
                                "{} {reading} {} {word}",
                                quota.label,
                                quota.unit.as_deref().unwrap_or("requests")
                            )
                        };
                        return TrayMetric {
                            value: reading,
                            detail,
                            gauge: used_fraction.map(|used_fraction| TrayGauge {
                                display_fraction: match display {
                                    UsageDisplay::Used => used_fraction,
                                    UsageDisplay::Left => 1.0 - used_fraction,
                                },
                                #[cfg(any(not(target_os = "macos"), test))]
                                remaining_fraction: 1.0 - used_fraction,
                            }),
                        };
                    }
                }
                let used_fraction = (quota.used_percent / 100.0).clamp(0.0, 1.0);
                let display_fraction = match display {
                    UsageDisplay::Used => used_fraction,
                    UsageDisplay::Left => 1.0 - used_fraction,
                };
                let percent = display_fraction * 100.0;
                let word = match display {
                    UsageDisplay::Used => "used",
                    UsageDisplay::Left => "left",
                };
                // Providers such as OpenCode Go report floored whole percentages, so a `0%`
                // reading covers every value below one percent.
                let value = if definition.floored_percent && quota.used_percent < 1.0 {
                    match display {
                        UsageDisplay::Used => "<1%".to_owned(),
                        UsageDisplay::Left => ">99%".to_owned(),
                    }
                } else {
                    format!("{percent:.0}%")
                };
                TrayMetric {
                    value: value.clone(),
                    detail: format!("{} {} {word}", quota.label, value),
                    gauge: Some(TrayGauge {
                        display_fraction,
                        #[cfg(any(not(target_os = "macos"), test))]
                        remaining_fraction: 1.0 - used_fraction,
                    }),
                }
            })
    };
    match &definition.source {
        MetricSource::Quota { source_id, .. } => quota(source_id),
        MetricSource::QuotaOrValue { source_id, .. } => {
            quota(source_id).or_else(|| value_metric(snapshot, source_id, tray.suffix.as_deref()))
        }
        MetricSource::Value { source_id } => {
            value_metric(snapshot, source_id, tray.suffix.as_deref())
        }
        MetricSource::Status { source_id } => status_metric(snapshot, source_id),
        MetricSource::Usage { period } => {
            usage_metric(&definition.label, usage_period(snapshot, *period))
        }
        MetricSource::CreditPackages => None,
        MetricSource::NearestCreditPackage { source_id } => {
            value_metric(snapshot, source_id, tray.suffix.as_deref())
        }
        MetricSource::Trend => None,
    }
}

fn usage_period(snapshot: &ProviderSnapshot, period: UsagePeriodSelection) -> Option<&UsagePeriod> {
    match period {
        UsagePeriodSelection::Today => snapshot.usage.today.as_ref(),
        UsagePeriodSelection::Yesterday => snapshot.usage.yesterday.as_ref(),
        UsagePeriodSelection::Last30Days => snapshot.usage.last_30_days.as_ref(),
    }
}

fn status_metric(snapshot: &ProviderSnapshot, source_id: &str) -> Option<TrayMetric> {
    let metric = snapshot
        .status_metrics
        .iter()
        .find(|metric| metric.id == source_id)?;
    Some(TrayMetric {
        value: metric.text.clone(),
        detail: format!("{} {}", metric.label, metric.text),
        gauge: None,
    })
}

fn value_metric(
    snapshot: &ProviderSnapshot,
    source_id: &str,
    tray_suffix: Option<&str>,
) -> Option<TrayMetric> {
    let metric = snapshot
        .value_metrics
        .iter()
        .find(|metric| metric.id == source_id)?;
    let value = metric
        .values
        .iter()
        .map(format_tray_value)
        .collect::<Vec<_>>()
        .join(" · ");
    let detail = metric
        .values
        .iter()
        .map(format_detail_value)
        .collect::<Vec<_>>()
        .join(" · ");
    let value = tray_suffix
        .map(|suffix| format!("{value} {suffix}"))
        .unwrap_or(value);
    Some(TrayMetric {
        value,
        detail: format!("{} {detail}", metric.label),
        gauge: None,
    })
}

fn format_tray_value(value: &MetricValue) -> String {
    if is_credit_unit(value.label.as_deref()) {
        return format!(
            "{CREDIT_SYMBOL}{}",
            format_currency_number(value.number, true)
        );
    }
    let number = match value.kind {
        MetricValueKind::Dollars => format!("${:.0}", value.number),
        MetricValueKind::Count => format_tokens(value.number.max(0.0) as u64),
        MetricValueKind::Currency => format_currency(value.number, value.label.as_deref(), true),
    };
    if value.kind == MetricValueKind::Currency {
        number
    } else {
        value
            .label
            .as_deref()
            .map(|label| format!("{number} {label}"))
            .unwrap_or(number)
    }
}

fn format_detail_value(value: &MetricValue) -> String {
    if is_credit_unit(value.label.as_deref()) {
        return format!(
            "{CREDIT_SYMBOL}{}",
            format_currency_number(value.number, false)
        );
    }
    let number = match value.kind {
        MetricValueKind::Dollars => format!(
            "${}",
            trim_decimal(value.number, amount_fraction_digits(value.number))
        ),
        MetricValueKind::Count => format!("{:.0}", value.number),
        MetricValueKind::Currency => format_currency(value.number, value.label.as_deref(), false),
    };
    if value.kind == MetricValueKind::Currency {
        number
    } else {
        value
            .label
            .as_deref()
            .map(|label| format!("{number} {label}"))
            .unwrap_or(number)
    }
}

/// The marker every credit reading carries, in the slot `¥`/`$` occupy for
/// money. Mirrors `CREDITS_SYMBOL` in `src/lib/metricFormat.ts`.
const CREDIT_SYMBOL: char = '✦';

/// Units that are points rather than money or a plain count. Mirrors
/// `isCreditUnit()` in `src/lib/metricFormat.ts`, `积分` included because
/// `t('units.credits')` is a member of the same set.
fn is_credit_unit(unit: Option<&str>) -> bool {
    let Some(unit) = unit.map(str::trim) else {
        return false;
    };
    ["credits", "credit", "points", "point"]
        .iter()
        .any(|known| unit.eq_ignore_ascii_case(known))
        || unit == "积分"
}

fn format_currency(number: f64, currency: Option<&str>, compact: bool) -> String {
    let number = format_currency_number(number, compact);
    let currency = currency.map(str::to_ascii_uppercase);
    match currency.as_deref() {
        Some("CNY" | "JPY") if compact => format!("¥{number}"),
        Some("USD") if compact => format!("${number}"),
        Some("EUR") if compact => format!("€{number}"),
        Some("GBP") if compact => format!("£{number}"),
        Some(currency) => format!("{number} {currency}"),
        None => number,
    }
}

/// Mirrors `fractionDigits()` in `src/lib/metricFormat.ts` so the menubar reads
/// the same as the dashboard: amounts keep one decimal place, and only a
/// non-zero value below half a tenth keeps extra digits, which stops a real
/// balance from being printed as `0`.
fn amount_fraction_digits(number: f64) -> usize {
    let magnitude = number.abs();
    if magnitude == 0.0 || magnitude >= 0.05 {
        return 1;
    }
    let leading_zeros = (-magnitude.log10()).ceil().max(0.0) as usize;
    (leading_zeros + 2).clamp(1, 20)
}

fn format_currency_number(number: f64, compact: bool) -> String {
    if !number.is_finite() {
        return "NA".to_owned();
    }
    let magnitude = number.abs();
    if compact && magnitude >= 1_000_000_000.0 {
        return trim_decimal(number / 1_000_000_000.0, 1) + "B";
    }
    if compact && magnitude >= 1_000_000.0 {
        return trim_decimal(number / 1_000_000.0, 1) + "M";
    }
    if compact && magnitude >= 1_000.0 {
        return trim_decimal(number / 1_000.0, 1) + "K";
    }
    trim_decimal(number, amount_fraction_digits(number))
}

fn trim_decimal(number: f64, precision: usize) -> String {
    let formatted = format!("{number:.precision$}");
    if formatted == "-0" {
        return "0".to_owned();
    }
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn usage_metric(label: &str, period: Option<&UsagePeriod>) -> Option<TrayMetric> {
    let period = period?;
    let value = period
        .estimated_cost_usd
        .map(|cost| format!("${}", trim_decimal(cost, amount_fraction_digits(cost))))
        .unwrap_or_else(|| format_tokens(period.tokens));
    let detail = format!("{label} {value}");
    Some(TrayMetric {
        value,
        detail,
        gauge: None,
    })
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn mark_icon() -> Image<'static> {
    Image::from_bytes(include_bytes!("../icons/32x32.png"))
        .expect("bundled Quota01 tray mark must be a valid PNG")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chrono::Utc;

    use crate::{
        models::{
            MetricDefinition, MetricSection, MetricValue, MetricValueKind, ProviderSnapshot,
            ProviderViewState, QuotaWindow, SnapshotSource, StatusMetric, StatusTone, UsageHistory,
            ValueMetric,
        },
        providers::{codex, cursor, opencode, trae, workbuddy, ProviderRegistry},
        settings::default_settings,
    };

    use super::{
        format_tokens, mark_icon, pinned_provider_metrics, primary_gauge,
        requested_provider_entries, resolved_groups, NativeInstancePlatform, TrayGauge, TrayGroup,
        TrayMetric,
    };
    use crate::service::UsageViewState;

    #[test]
    fn bundled_tray_mark_decodes_at_the_expected_size() {
        let image = mark_icon();
        assert_eq!((image.width(), image.height()), (32, 32));
    }

    #[test]
    fn pinned_quota_metrics_resolve_in_layout_order() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "codex".into(),
            plan: None,
            quotas: vec![
                QuotaWindow {
                    id: "session".into(),
                    label: "Session".into(),
                    used_percent: 25.0,
                    resets_at: None,
                    period_seconds: 18_000,
                    format: crate::models::QuotaFormat::Percent,
                    used_value: None,
                    limit_value: None,
                    unit: None,
                    estimated: false,
                    source_note: None,
                },
                QuotaWindow {
                    id: "weekly".into(),
                    label: "Weekly".into(),
                    used_percent: 60.0,
                    resets_at: None,
                    period_seconds: 604_800,
                    format: crate::models::QuotaFormat::Percent,
                    used_value: None,
                    limit_value: None,
                    unit: None,
                    estimated: false,
                    source_note: None,
                },
            ],
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let provider_state = ProviderViewState {
            snapshot: Some(snapshot),
            source: SnapshotSource::Live,
            ..ProviderViewState::default()
        };
        let state = UsageViewState {
            providers: [("codex".into(), provider_state)].into_iter().collect(),
            last_full_refresh_at: None,
            next_refresh_at: None,
        };
        let catalog = ProviderRegistry::from_definitions(vec![codex::definition()]).unwrap();
        let groups = resolved_groups(
            &state,
            &default_settings(&catalog, &HashSet::from(["codex".to_owned()])),
            &catalog,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].provider_id, "codex");
        assert_eq!(groups[0].metrics.len(), 2);
        assert_eq!(groups[0].metrics[0].value, "75%");
        assert_eq!(groups[0].metrics[1].value, "40%");
        assert_eq!(
            groups[0].metrics[0].gauge,
            Some(TrayGauge {
                display_fraction: 0.75,
                remaining_fraction: 0.75,
            })
        );

        let mut dashboard_hidden = default_settings(&catalog, &HashSet::from(["codex".to_owned()]));
        dashboard_hidden.providers[0].metrics[0].enabled = false;
        let hidden_groups = resolved_groups(&state, &dashboard_hidden, &catalog);
        assert_eq!(hidden_groups[0].metrics[0].value, "75%");

        let mut used_settings = default_settings(&catalog, &HashSet::from(["codex".to_owned()]));
        used_settings.usage_display = crate::models::UsageDisplay::Used;
        let used_groups = resolved_groups(&state, &used_settings, &catalog);
        assert_eq!(
            used_groups[0].metrics[0].gauge,
            Some(TrayGauge {
                display_fraction: 0.25,
                remaining_fraction: 0.75,
            })
        );
        assert_eq!(used_groups[0].metrics[0].value, "25%");
    }

    #[test]
    fn count_quota_display_changes_text_and_fill_but_not_status_fraction() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "cursor".into(),
            plan: None,
            quotas: vec![QuotaWindow {
                id: "requests".into(),
                label: "Requests".into(),
                used_percent: 25.0,
                resets_at: None,
                period_seconds: 2_592_000,
                format: crate::models::QuotaFormat::Count,
                used_value: Some(25.0),
                limit_value: Some(100.0),
                unit: Some("searches".into()),
                estimated: false,
                source_note: None,
            }],
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let catalog = ProviderRegistry::from_definitions(vec![cursor::definition()]).unwrap();
        let definition = catalog.metric("cursor.requests").unwrap();

        let left =
            super::tray_metric(definition, &snapshot, crate::models::UsageDisplay::Left).unwrap();
        let used =
            super::tray_metric(definition, &snapshot, crate::models::UsageDisplay::Used).unwrap();

        assert_eq!(left.value, "75");
        assert_eq!(left.detail, "Requests 75 searches left");
        assert_eq!(used.value, "25");
        assert_eq!(used.detail, "Requests 25 searches used");
        assert_eq!(
            left.gauge,
            Some(TrayGauge {
                display_fraction: 0.75,
                remaining_fraction: 0.75,
            })
        );
        assert_eq!(
            used.gauge,
            Some(TrayGauge {
                display_fraction: 0.25,
                remaining_fraction: 0.75,
            })
        );
    }

    #[test]
    fn floored_percent_quotas_render_sub_one_percent_on_the_menubar() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "opencode".into(),
            plan: Some("Go".into()),
            quotas: vec![QuotaWindow {
                id: "session".into(),
                label: "Session (5h)".into(),
                used_percent: 0.0,
                resets_at: None,
                period_seconds: 18_000,
                format: crate::models::QuotaFormat::Percent,
                used_value: None,
                limit_value: None,
                unit: None,
                estimated: false,
                source_note: None,
            }],
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let catalog =
            ProviderRegistry::from_definitions(vec![codex::definition(), opencode::definition()])
                .unwrap();
        let definition = catalog.metric("opencode.session").unwrap();

        let left =
            super::tray_metric(definition, &snapshot, crate::models::UsageDisplay::Left).unwrap();
        let used =
            super::tray_metric(definition, &snapshot, crate::models::UsageDisplay::Used).unwrap();

        assert_eq!(left.value, ">99%");
        assert_eq!(left.detail, "Session (5h) >99% left");
        assert_eq!(used.value, "<1%");
        assert_eq!(used.detail, "Session (5h) <1% used");
    }

    #[test]
    fn unavailable_pinned_metrics_fall_back_to_na_instead_of_disappearing() {
        // Codex defaults pin session + weekly, but the snapshot only carries
        // weekly this round: session must stay visible as NA on both the
        // macOS menubar and Windows taskband paths instead of being dropped.
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "codex".into(),
            plan: None,
            quotas: vec![QuotaWindow {
                id: "weekly".into(),
                label: "Weekly".into(),
                used_percent: 60.0,
                resets_at: None,
                period_seconds: 604_800,
                format: crate::models::QuotaFormat::Percent,
                used_value: None,
                limit_value: None,
                unit: None,
                estimated: false,
                source_note: None,
            }],
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let provider_state = ProviderViewState {
            snapshot: Some(snapshot),
            source: SnapshotSource::Live,
            ..ProviderViewState::default()
        };
        let state = UsageViewState {
            providers: [("codex".into(), provider_state)].into_iter().collect(),
            last_full_refresh_at: None,
            next_refresh_at: None,
        };
        let catalog = ProviderRegistry::from_definitions(vec![codex::definition()]).unwrap();
        let settings = default_settings(&catalog, &HashSet::from(["codex".to_owned()]));

        let groups = resolved_groups(&state, &settings, &catalog);
        assert_eq!(groups[0].metrics.len(), 2);
        assert_eq!(groups[0].metrics[0].value, "NA");
        assert_eq!(groups[0].metrics[1].value, "40%");

        let provider = settings
            .providers
            .iter()
            .find(|provider| provider.id == "codex")
            .unwrap()
            .clone();
        let pinned = pinned_provider_metrics(&state, &provider, &settings, &catalog);
        assert_eq!(pinned.len(), 2);
        assert_eq!(pinned[0].value, "NA");
        assert_eq!(pinned[1].value, "40%");
    }

    #[test]
    fn requested_provider_instances_and_placeholders_do_not_depend_on_snapshots() {
        let catalog = ProviderRegistry::from_definitions(vec![codex::definition()]).unwrap();
        let mut settings = default_settings(&catalog, &HashSet::from(["codex".to_owned()]));
        assert_eq!(
            requested_provider_entries(&settings, &catalog, NativeInstancePlatform::MacOS),
            ["codex"]
        );
        let provider = settings
            .providers
            .iter()
            .find(|item| item.id == "codex")
            .unwrap();
        let metrics =
            pinned_provider_metrics(&UsageViewState::default(), provider, &settings, &catalog);
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].value, "--");
        assert_eq!(metrics[1].value, "--");

        settings.providers[0].enabled = false;
        assert!(
            requested_provider_entries(&settings, &catalog, NativeInstancePlatform::MacOS)
                .is_empty()
        );
    }

    #[test]
    fn token_fallback_stays_compact() {
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(12_340), "12.3K");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn gauge_uses_the_first_pinned_quota_metric() {
        let groups = vec![TrayGroup {
            provider_id: "codex".into(),
            metrics: vec![
                TrayMetric {
                    value: "10".into(),
                    detail: "Credits 10".into(),
                    gauge: None,
                },
                TrayMetric {
                    value: "40%".into(),
                    detail: "Session 40% left".into(),
                    gauge: Some(TrayGauge {
                        display_fraction: 0.4,
                        remaining_fraction: 0.4,
                    }),
                },
                TrayMetric {
                    value: "80%".into(),
                    detail: "Weekly 80% left".into(),
                    gauge: Some(TrayGauge {
                        display_fraction: 0.8,
                        remaining_fraction: 0.8,
                    }),
                },
            ],
        }];
        assert_eq!(
            primary_gauge(&groups),
            Some(TrayGauge {
                display_fraction: 0.4,
                remaining_fraction: 0.4,
            })
        );
    }

    #[test]
    fn pinned_value_metrics_keep_numeric_values_outside_quota_bars() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "codex".into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: vec![ValueMetric {
                id: "credits".into(),
                label: "Extra Usage".into(),
                values: vec![
                    MetricValue {
                        number: 32.84,
                        kind: MetricValueKind::Dollars,
                        label: None,
                        estimated: true,
                    },
                    MetricValue {
                        number: 821.0,
                        kind: MetricValueKind::Count,
                        label: Some("credits".into()),
                        estimated: false,
                    },
                ],
                expiries_at: Vec::new(),
            }],
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let catalog = ProviderRegistry::from_definitions(vec![codex::definition()]).unwrap();
        let metric = super::tray_metric(
            catalog.metric("codex.credits").unwrap(),
            &snapshot,
            crate::models::UsageDisplay::Left,
        )
        .unwrap();
        assert_eq!(metric.value, "$33 · ✦821");
        assert_eq!(metric.detail, "Extra Usage $32.8 · ✦821");
        assert_eq!(metric.gauge, None);
    }

    #[test]
    fn workbuddy_nearest_expiring_tray_value_keeps_the_credit_marker() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "workbuddy-cn".into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: vec![ValueMetric {
                id: "nearestExpiring".into(),
                label: "近期到期的积分包".into(),
                values: vec![MetricValue {
                    number: 71.38,
                    kind: MetricValueKind::Count,
                    label: Some("credits".into()),
                    estimated: false,
                }],
                expiries_at: Vec::new(),
            }],
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let catalog =
            ProviderRegistry::from_definitions(vec![workbuddy::definition(), codex::definition()])
                .unwrap();
        let metric = super::tray_metric(
            catalog.metric("workbuddy-cn.nearestExpiring").unwrap(),
            &snapshot,
            crate::models::UsageDisplay::Left,
        )
        .unwrap();

        assert_eq!(metric.value, "✦71.4");
    }

    #[test]
    fn workbuddy_credits_tray_value_tracks_the_remaining_balance() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "workbuddy-cn".into(),
            plan: None,
            quotas: vec![QuotaWindow {
                id: "credits".into(),
                label: "Credits".into(),
                used_percent: 69.24826527662142,
                resets_at: None,
                period_seconds: 0,
                format: crate::models::QuotaFormat::Count,
                used_value: Some(2295.57999392),
                limit_value: Some(3315.0),
                unit: Some("credits".into()),
                estimated: false,
                source_note: None,
            }],
            value_metrics: vec![ValueMetric {
                id: "balance".into(),
                label: "Balance".into(),
                values: vec![MetricValue {
                    number: 1019.42000608,
                    kind: MetricValueKind::Count,
                    label: Some("credits".into()),
                    estimated: false,
                }],
                expiries_at: Vec::new(),
            }],
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let catalog =
            ProviderRegistry::from_definitions(vec![workbuddy::definition(), codex::definition()])
                .unwrap();

        let metric = super::tray_metric(
            catalog.metric("workbuddy-cn.credits").unwrap(),
            &snapshot,
            crate::models::UsageDisplay::Used,
        )
        .unwrap();

        assert_eq!(metric.value, "✦1K");
    }

    #[test]
    fn credit_windows_mark_the_number_and_drop_the_unit_word() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "trae-cn".into(),
            plan: None,
            quotas: vec![QuotaWindow {
                id: "credits".into(),
                label: "Credits".into(),
                used_percent: 54.7,
                resets_at: None,
                period_seconds: 0,
                format: crate::models::QuotaFormat::Count,
                used_value: Some(821.0),
                limit_value: Some(1500.0),
                unit: Some("credits".into()),
                estimated: false,
                source_note: None,
            }],
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let catalog =
            ProviderRegistry::from_definitions(vec![trae::definition(), codex::definition()])
                .unwrap();
        let definition = catalog.metric("trae-cn.credits").unwrap();

        let left =
            super::tray_metric(definition, &snapshot, crate::models::UsageDisplay::Left).unwrap();
        let used =
            super::tray_metric(definition, &snapshot, crate::models::UsageDisplay::Used).unwrap();

        assert_eq!(left.value, "✦679");
        assert_eq!(left.detail, "Credits ✦679 left");
        assert_eq!(used.value, "✦821");
        assert_eq!(used.detail, "Credits ✦821 used");
        // The window keeps its fraction: only the reading text carries the marker.
        assert_eq!(
            left.gauge,
            Some(TrayGauge {
                display_fraction: 1.0 - 821.0 / 1500.0,
                remaining_fraction: 1.0 - 821.0 / 1500.0,
            })
        );
    }

    #[test]
    fn pinned_status_metrics_keep_text_and_never_create_a_gauge() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "grok".into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: vec![StatusMetric {
                id: "payAsYouGo".into(),
                label: "Extra Usage".into(),
                text: "2500 cap".into(),
                tone: StatusTone::Positive,
                subtitle: None,
            }],
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let definition = MetricDefinition::status(
            "grok.payAsYouGo",
            "Extra Usage",
            "payAsYouGo",
            true,
            MetricSection::AlwaysVisible,
            true,
            "E",
        );

        let metric =
            super::tray_metric(&definition, &snapshot, crate::models::UsageDisplay::Left).unwrap();

        assert_eq!(metric.value, "2500 cap");
        assert_eq!(metric.detail, "Extra Usage 2500 cap");
        assert_eq!(metric.gauge, None);
        assert!(primary_gauge(&[TrayGroup {
            provider_id: "grok".into(),
            metrics: vec![metric],
        }])
        .is_none());
    }

    #[test]
    fn currency_values_keep_codes_in_details_and_symbols_on_the_tray() {
        let cny_balance = MetricValue {
            number: 110.0,
            kind: MetricValueKind::Currency,
            label: Some("CNY".into()),
            estimated: false,
        };
        let small_usd_spend = MetricValue {
            number: 0.0004,
            kind: MetricValueKind::Currency,
            label: Some("USD".into()),
            estimated: false,
        };
        let deepseek_balance = MetricValue {
            number: 9.7134,
            kind: MetricValueKind::Currency,
            label: Some("CNY".into()),
            estimated: false,
        };

        assert_eq!(super::format_currency(110.0, Some("CNY"), true), "¥110");
        assert_eq!(super::format_currency(0.0004, Some("USD"), true), "$0.0004");
        assert_eq!(super::format_tray_value(&cny_balance), "¥110");
        assert_eq!(super::format_detail_value(&cny_balance), "110 CNY");
        assert_eq!(super::format_detail_value(&small_usd_spend), "0.0004 USD");
        assert_eq!(super::format_tray_value(&deepseek_balance), "¥9.7");
        assert_eq!(super::format_detail_value(&deepseek_balance), "9.7 CNY");
    }

    #[test]
    fn amounts_keep_one_decimal_unless_rounding_would_erase_them() {
        assert_eq!(super::amount_fraction_digits(0.0), 1);
        assert_eq!(super::amount_fraction_digits(0.05), 1);
        assert_eq!(super::amount_fraction_digits(9.7134), 1);
        assert_eq!(super::amount_fraction_digits(0.0004), 6);

        assert_eq!(
            super::format_currency(50.54, Some("CNY"), false),
            "50.5 CNY"
        );
        assert_eq!(super::format_currency(9.7134, Some("CNY"), true), "¥9.7");
        assert_eq!(
            super::format_currency(0.0004, Some("USD"), false),
            "0.0004 USD"
        );

        let usd_spend = MetricValue {
            number: 2059.07,
            kind: MetricValueKind::Dollars,
            label: None,
            estimated: false,
        };
        assert_eq!(super::format_detail_value(&usd_spend), "$2059.1");
    }

    #[test]
    fn credit_values_read_as_a_marker_instead_of_a_unit_word() {
        let credits = MetricValue {
            number: 71.38,
            kind: MetricValueKind::Count,
            label: Some("credits".into()),
            estimated: false,
        };
        let localized = MetricValue {
            label: Some("积分".into()),
            ..credits.clone()
        };
        let requests = MetricValue {
            number: 71.38,
            kind: MetricValueKind::Count,
            label: Some("requests".into()),
            estimated: false,
        };

        assert_eq!(super::format_tray_value(&credits), "✦71.4");
        assert_eq!(super::format_detail_value(&credits), "✦71.4");
        // `t('units.credits')` is 积分 in the Chinese pack, so both spellings
        // have to resolve to the same reading.
        assert_eq!(super::format_tray_value(&localized), "✦71.4");
        assert_eq!(super::format_detail_value(&localized), "✦71.4");
        // A count with a unit of its own keeps the reading it always had.
        assert_eq!(super::format_tray_value(&requests), "71 requests");
        assert_eq!(super::format_detail_value(&requests), "71 requests");
        assert!(!super::is_credit_unit(Some("searches")));
        assert!(!super::is_credit_unit(None));
    }
}
