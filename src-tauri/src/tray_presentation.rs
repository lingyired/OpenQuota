use tauri::{image::Image, AppHandle};

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

const TRAY_ID: &str = "usage01-tray";

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

#[derive(Debug, Clone, PartialEq)]
struct TrayGroup {
    #[cfg(test)]
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

    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let groups = resolved_groups(state, settings, registry);
    let tooltip = if groups.is_empty() {
        "Usage01".to_owned()
    } else {
        format!(
            "Usage01\n{}",
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

    #[cfg(not(target_os = "macos"))]
    {
        let icon = primary_gauge(&groups)
            .map(|gauge| tray_icon::render_gauge(gauge.display_fraction, gauge.remaining_fraction))
            .unwrap_or_else(mark_icon);
        if tray.set_icon(Some(icon)).is_err() {
            crate::app_warn!("tray", "tray icon update failed");
        }
    }

    #[cfg(target_os = "macos")]
    {
        // 菜单栏的指标内容已交给 multiline-menubar 插件逐 provider 渲染；
        // 托盘/状态项只保留应用 mark + 右键菜单（settings/quit），
        // 对应 Windows 的「系统托盘 + taskband」双轨结构。
        if tray.set_title(Some("")).is_err() {
            crate::app_warn!("tray", "macOS menu bar title clear failed");
        }
        if tray
            .set_icon_with_as_template(Some(mark_icon()), true)
            .is_err()
        {
            crate::app_warn!("tray", "macOS menu bar icon update failed");
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
                #[cfg(test)]
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
    let Some(snapshot) = state
        .providers
        .get(&provider.id)
        .and_then(|state| state.snapshot.as_ref())
    else {
        return Vec::new();
    };
    provider
        .metrics
        .iter()
        .filter(|metric| metric.pinned)
        .filter_map(|metric| {
            let definition = registry.metric(&metric.id)?;
            let tray = definition.tray.as_ref()?;
            let value = tray_metric(definition, snapshot, settings.usage_display)
                .map(|resolved| resolved.value)
                .unwrap_or_else(|| "NA".to_owned());
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
                        let unit = quota.unit.as_deref().unwrap_or("requests");
                        return TrayMetric {
                            value: format!("{value:.0}"),
                            detail: format!("{} {value:.0} {unit} {word}", quota.label),
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
    let number = match value.kind {
        MetricValueKind::Dollars => format!("${:.0}", value.number),
        MetricValueKind::Count => format_tokens(value.number.max(0.0) as u64),
    };
    value
        .label
        .as_deref()
        .map(|label| format!("{number} {label}"))
        .unwrap_or(number)
}

fn format_detail_value(value: &MetricValue) -> String {
    let number = match value.kind {
        MetricValueKind::Dollars => format!("${:.2}", value.number),
        MetricValueKind::Count => format!("{:.0}", value.number),
    };
    value
        .label
        .as_deref()
        .map(|label| format!("{number} {label}"))
        .unwrap_or(number)
}

fn usage_metric(label: &str, period: Option<&UsagePeriod>) -> Option<TrayMetric> {
    let period = period?;
    let value = period
        .estimated_cost_usd
        .map(|value| format!("${value:.2}"))
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

fn mark_icon() -> Image<'static> {
    Image::from_bytes(include_bytes!("../icons/32x32.png"))
        .expect("bundled Usage01 tray mark must be a valid PNG")
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
        providers::{codex, cursor, opencode, workbuddy, ProviderRegistry},
        settings::default_settings,
    };

    use super::{
        format_tokens, pinned_provider_metrics, primary_gauge, resolved_groups, TrayGauge,
        TrayGroup, TrayMetric,
    };
    use crate::service::UsageViewState;

    #[test]
    fn bundled_tray_mark_decodes_at_the_expected_size() {
        let image = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
            .expect("bundled tray mark should decode");
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
    fn pinned_metrics_without_a_snapshot_do_not_leave_placeholder_content() {
        let catalog = ProviderRegistry::from_definitions(vec![codex::definition()]).unwrap();
        let settings = default_settings(&catalog, &HashSet::from(["codex".to_owned()]));
        assert!(resolved_groups(&UsageViewState::default(), &settings, &catalog).is_empty());
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
        assert_eq!(metric.value, "$33 · 821 credits");
        assert_eq!(metric.detail, "Extra Usage $32.84 · 821 credits");
        assert_eq!(metric.gauge, None);
    }

    #[test]
    fn workbuddy_nearest_expiring_tray_value_is_a_bare_count() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "workbuddy".into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: vec![ValueMetric {
                id: "nearestExpiring".into(),
                label: "近期到期的积分包".into(),
                values: vec![MetricValue {
                    number: 71.38,
                    kind: MetricValueKind::Count,
                    label: None,
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
            catalog.metric("workbuddy.nearestExpiring").unwrap(),
            &snapshot,
            crate::models::UsageDisplay::Left,
        )
        .unwrap();

        assert_eq!(metric.value, "71");
    }

    #[test]
    fn workbuddy_credits_tray_value_tracks_the_remaining_balance() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "workbuddy".into(),
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
                    label: None,
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
            catalog.metric("workbuddy.credits").unwrap(),
            &snapshot,
            crate::models::UsageDisplay::Used,
        )
        .unwrap();

        assert_eq!(metric.value, "1.0K");
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
}
