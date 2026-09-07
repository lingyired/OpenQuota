//! Windows 任务栏 taskband 集成（tauri-plugin-multiline-taskband）。
//!
//! 职责：根据 `AppSettings` + `UsageViewState` 对账每个已启用监控在任务栏上
//! 的两行文本标签实例 —— 创建 / 更新 / 隐藏 / 移除，并监听实例点击事件
//! （左键 → 打开主窗口 popup 并滚动到对应监控）。
//!
//! 文本组装逻辑为纯函数（可在任意平台单测），所有调用插件的代码
//! 均以 `#[cfg(target_os = "windows")]` 隔离，非 Windows 零影响。

use crate::models::TaskbandLayout;
#[cfg(target_os = "windows")]
use crate::models::{TaskbandColorStyle, TaskbandPreferences, TaskbandSide};
#[cfg(target_os = "windows")]
use crate::providers::ProviderRegistry;
#[cfg(target_os = "windows")]
use crate::service::UsageViewState;
#[cfg(target_os = "windows")]
use crate::tray_presentation::resolved_provider_metrics;
use crate::tray_presentation::ResolvedTrayMetric;
#[cfg(target_os = "windows")]
use crate::window::MAIN_WINDOW;
#[cfg(target_os = "windows")]
use std::collections::{HashMap, HashSet};
#[cfg(target_os = "windows")]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use tauri::{AppHandle, Emitter, EventId, Listener, Manager};

#[cfg(target_os = "windows")]
use tauri_plugin_multiline_taskband::{ColorStyle, MultilineTaskbandExt, Side};

/// 与前端 `providerIconPaths.ts` 保持一致的品牌色表。无品牌色的 provider
/// 自动使用系统任务栏文字色（`Default`）。
#[cfg(target_os = "windows")]
const BRAND_COLORS: &[(&str, &str)] = &[
    ("antigravity", "#4285F4"),
    ("claude", "#DE7356"),
    ("kimi", "#1783FF"),
    ("minimax", "#E2167E"),
];

#[cfg(target_os = "windows")]
fn brand_color(provider_id: &str) -> Option<&'static str> {
    BRAND_COLORS
        .iter()
        .find(|(id, _)| provider_id.starts_with(id))
        .map(|(_, color)| *color)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaskbandPresentation {
    pub top_line: String,
    pub bottom_line: String,
    pub bottom_visible: bool,
}

/// 组装单个 provider 的两行任务栏文本。
///
/// - 第一行：`{short_name} {槽位1值}`（无值则只显示缩写图标）。
/// - 第二行：`{短标签} {槽位2值} · {短标签} {槽位3值}`（`show_labels` 控制
///   是否带短标签；槽位缺失自动跳过；两个槽位都为空则隐藏第二行）。
///
/// `explicit` 表示该 provider 是否在 `taskband_providers` 中显式配置：
/// - 显式配置：`slot` 为 `None` 或空串表示"不显示"，`Some(id)` 为指定指标。
/// - 未配置：自动取第 1/2/3 个候选指标（无 `slot` 概念）。
pub(crate) fn assemble_taskband_text(
    short_name: &str,
    layout: &TaskbandLayout,
    metrics: &[ResolvedTrayMetric],
    explicit: bool,
) -> TaskbandPresentation {
    let resolve = |slot: &Option<String>, fallback: usize| -> Option<&ResolvedTrayMetric> {
        match (explicit, slot) {
            (true, Some(id)) if !id.is_empty() => metrics.iter().find(|metric| &metric.id == id),
            (true, _) => None,
            (false, _) => metrics.get(fallback),
        }
    };
    let top_value = resolve(&layout.slot_top, 0).map(|metric| metric.value.as_str());
    let top_line = match top_value {
        Some(value) => format!("{short_name} {value}"),
        None => short_name.to_owned(),
    };

    let bottom_parts: Vec<String> = [
        resolve(&layout.slot_bottom, 1),
        resolve(&layout.slot_bottom_2, 2),
    ]
    .into_iter()
    .flatten()
    .map(|metric| {
        if layout.show_labels {
            format!("{} {}", metric.short_label, metric.value)
        } else {
            metric.value.clone()
        }
    })
    .collect();
    TaskbandPresentation {
        top_line,
        bottom_line: bottom_parts.join(" · "),
        bottom_visible: !bottom_parts.is_empty(),
    }
}

/// 每个已创建 taskband 实例上次应用的配置，用于 diff 避免重复调用 set_*。
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq)]
struct AppliedConfig {
    side: TaskbandSide,
    order: u64,
    text: (String, String),
    lines_visible: (bool, bool),
    top_color: TaskbandColorStyle,
    bottom_color: TaskbandColorStyle,
    top_bold: bool,
    bottom_bold: bool,
    top_size: f64,
    bottom_size: f64,
    top_align: i32,
    bottom_align: i32,
    padding: (i32, i32),
    visible: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq)]
struct GlobalConfig {
    margin: i32,
    edge_margin_left: i32,
    edge_margin_right: i32,
}

#[cfg(target_os = "windows")]
pub(crate) struct TaskbandState {
    created: Mutex<HashMap<String, AppliedConfig>>,
    click_listeners: Mutex<HashMap<String, EventId>>,
    global: Mutex<Option<GlobalConfig>>,
}

#[cfg(target_os = "windows")]
impl Default for TaskbandState {
    fn default() -> Self {
        Self {
            created: Mutex::new(HashMap::new()),
            click_listeners: Mutex::new(HashMap::new()),
            global: Mutex::new(None),
        }
    }
}

#[cfg(target_os = "windows")]
impl TaskbandState {
    fn apply_global(&self, app: &AppHandle, prefs: &TaskbandPreferences) {
        let mut stored = self.global.lock().unwrap_or_else(|e| e.into_inner());
        let next = GlobalConfig {
            margin: prefs.margin,
            edge_margin_left: prefs.edge_margin_left,
            edge_margin_right: prefs.edge_margin_right,
        };
        if stored.as_ref() == Some(&next) {
            return;
        }
        let tb = app.multiline_taskband();
        let _ = tb.set_auto_popup(false);
        let _ = tb.set_margin(next.margin);
        let _ = tb.set_edge_margins(Some(next.edge_margin_left), Some(next.edge_margin_right));
        *stored = Some(next);
    }

    fn apply_instance(&self, app: &AppHandle, id: &str, config: AppliedConfig) {
        let tb = app.multiline_taskband();
        let mut created = self.created.lock().unwrap_or_else(|e| e.into_inner());
        match created.get(id) {
            None => {
                let side = match config.side {
                    TaskbandSide::Left => Side::Left,
                    TaskbandSide::Right => Side::Right,
                };
                let _ = tb.create(id.to_string(), side);
                let _ = tb.set_order(id.to_string(), config.order);
                let _ = tb.set_text(id.to_string(), config.text.0.clone(), config.text.1.clone());
                let _ = tb.set_line_visible(
                    id.to_string(),
                    config.lines_visible.0,
                    config.lines_visible.1,
                );
                let _ = tb.set_colors(
                    id.to_string(),
                    to_plugin_color(&config.top_color, id),
                    to_plugin_color(&config.bottom_color, ""),
                );
                let _ = tb.set_bold(id.to_string(), config.top_bold, config.bottom_bold);
                let _ = tb.set_font_sizes(id.to_string(), config.top_size, config.bottom_size);
                let _ = tb.set_alignment(id.to_string(), config.top_align, config.bottom_align);
                let _ = tb.set_padding(id.to_string(), config.padding.0, config.padding.1);
                let _ = tb.set_visible(id.to_string(), config.visible);
            }
            Some(previous) => {
                if previous.side != config.side {
                    let side = match config.side {
                        TaskbandSide::Left => Side::Left,
                        TaskbandSide::Right => Side::Right,
                    };
                    let _ = tb.set_side(id.to_string(), side);
                }
                if previous.order != config.order {
                    let _ = tb.set_order(id.to_string(), config.order);
                }
                if previous.text != config.text {
                    let _ =
                        tb.set_text(id.to_string(), config.text.0.clone(), config.text.1.clone());
                }
                if previous.lines_visible != config.lines_visible {
                    let _ = tb.set_line_visible(
                        id.to_string(),
                        config.lines_visible.0,
                        config.lines_visible.1,
                    );
                }
                if previous.top_color != config.top_color
                    || previous.bottom_color != config.bottom_color
                {
                    let _ = tb.set_colors(
                        id.to_string(),
                        to_plugin_color(&config.top_color, id),
                        to_plugin_color(&config.bottom_color, ""),
                    );
                }
                if previous.top_bold != config.top_bold
                    || previous.bottom_bold != config.bottom_bold
                {
                    let _ = tb.set_bold(id.to_string(), config.top_bold, config.bottom_bold);
                }
                if previous.top_size != config.top_size
                    || previous.bottom_size != config.bottom_size
                {
                    let _ = tb.set_font_sizes(id.to_string(), config.top_size, config.bottom_size);
                }
                if previous.top_align != config.top_align
                    || previous.bottom_align != config.bottom_align
                {
                    let _ = tb.set_alignment(id.to_string(), config.top_align, config.bottom_align);
                }
                if previous.padding != config.padding {
                    let _ = tb.set_padding(id.to_string(), config.padding.0, config.padding.1);
                }
                if previous.visible != config.visible {
                    let _ = tb.set_visible(id.to_string(), config.visible);
                }
            }
        }
        created.insert(id.to_string(), config);
    }

    fn remove_instance(&self, app: &AppHandle, id: &str) {
        let _ = app.multiline_taskband().remove(id.to_string());
        self.created
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        if let Some(listener_id) = self
            .click_listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
        {
            app.unlisten(listener_id);
        }
    }

    fn remove_all(&self, app: &AppHandle) {
        let ids = self
            .created
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.remove_instance(app, &id);
        }
        *self.global.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn register_click_listener(&self, app: &AppHandle, id: &str) {
        let mut registered = self
            .click_listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if registered.contains_key(id) {
            return;
        }
        let event = format!("multiline-taskband://{id}//click");
        let listen_id = id.to_string();
        let listener_app = app.clone();
        let listener_id = app.listen(event, move |event| {
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) else {
                return;
            };
            if payload.get("button").and_then(|b| b.as_str()) != Some("left") {
                return;
            }
            let provider_id = payload
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or(&listen_id)
                .to_owned();
            if let Some(window) = listener_app.get_webview_window(MAIN_WINDOW) {
                crate::window::show_main_window(&window);
            }
            let _ = listener_app.emit("taskband-open", provider_id);
        });
        registered.insert(id.to_string(), listener_id);
    }
}

#[cfg(target_os = "windows")]
fn to_plugin_color(color: &TaskbandColorStyle, fallback_brand_provider: &str) -> ColorStyle {
    match color {
        TaskbandColorStyle::Solid { value } => ColorStyle::Solid {
            value: value.clone(),
        },
        TaskbandColorStyle::Default => {
            if let Some(color) = brand_color(fallback_brand_provider) {
                ColorStyle::Solid {
                    value: color.to_owned(),
                }
            } else {
                ColorStyle::Default
            }
        }
    }
}

/// 对账入口：根据设置 + 快照创建 / 更新 / 移除 taskband 实例。
/// 挂载点在 `tray_presentation::update()` 末尾（Windows）。
#[cfg(target_os = "windows")]
pub(crate) fn update(
    app: &AppHandle,
    state: &UsageViewState,
    settings: &AppSettings,
    registry: &ProviderRegistry,
) {
    let taskband = app.state::<TaskbandState>();
    let prefs = &settings.taskband;
    if !prefs.enabled {
        taskband.remove_all(app);
        return;
    }
    taskband.apply_global(app, prefs);

    let mut desired_ids = HashSet::new();
    for (index, provider) in settings.providers.iter().enumerate() {
        if !provider.enabled {
            continue;
        }
        let Some(definition) = registry.definition(&provider.id) else {
            continue;
        };
        let layout = settings
            .taskband_providers
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        if !layout.enabled {
            continue;
        }
        let explicit = settings.taskband_providers.contains_key(&provider.id);
        let side = layout.side.unwrap_or(prefs.default_side);
        let metrics = resolved_provider_metrics(state, provider, settings, registry);
        let presentation =
            assemble_taskband_text(&definition.short_name, &layout, &metrics, explicit);
        let has_snapshot = state
            .providers
            .get(&provider.id)
            .and_then(|p| p.snapshot.as_ref())
            .is_some();
        let config = AppliedConfig {
            side,
            order: index as u64,
            text: (
                presentation.top_line.clone(),
                presentation.bottom_line.clone(),
            ),
            lines_visible: (true, presentation.bottom_visible),
            top_color: layout
                .top_color
                .clone()
                .unwrap_or(TaskbandColorStyle::Default),
            bottom_color: layout
                .bottom_color
                .clone()
                .unwrap_or(TaskbandColorStyle::Default),
            top_bold: layout.top_bold,
            bottom_bold: layout.bottom_bold,
            top_size: layout.top_size,
            bottom_size: layout.bottom_size,
            top_align: layout.top_align,
            bottom_align: layout.bottom_align,
            padding: (layout.padding_left, layout.padding_right),
            visible: has_snapshot,
        };
        taskband.apply_instance(app, &provider.id, config);
        taskband.register_click_listener(app, &provider.id);
        desired_ids.insert(provider.id.clone());
    }

    let stale = taskband
        .created
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .keys()
        .cloned()
        .filter(|id| !desired_ids.contains(id))
        .collect::<Vec<_>>();
    for id in stale {
        taskband.remove_instance(app, &id);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::{
        models::{QuotaFormat, QuotaWindow, SnapshotSource},
        providers::{codex, opencode, ProviderRegistry},
        settings::default_settings,
        tray_presentation::ResolvedTrayMetric,
    };

    use super::{assemble_taskband_text, TaskbandLayout};

    fn layout() -> TaskbandLayout {
        TaskbandLayout::default()
    }

    fn resolved(
        registry: &ProviderRegistry,
        provider_id: &str,
        quotas: &[(&str, f64)],
    ) -> Vec<ResolvedTrayMetric> {
        use crate::tray_presentation::resolved_provider_metrics;
        let mut catalog_settings =
            default_settings(registry, &HashSet::from([provider_id.to_owned()]));
        catalog_settings.usage_display = crate::models::UsageDisplay::Used;
        let provider = catalog_settings
            .providers
            .iter()
            .find(|p| p.id == provider_id)
            .unwrap()
            .clone();
        let snapshot = crate::models::ProviderSnapshot {
            provider_id: provider_id.into(),
            plan: None,
            quotas: quotas
                .iter()
                .map(|(id, percent)| QuotaWindow {
                    id: (*id).into(),
                    label: (*id).into(),
                    used_percent: *percent,
                    resets_at: None,
                    period_seconds: 0,
                    format: QuotaFormat::Percent,
                    used_value: None,
                    limit_value: None,
                    unit: None,
                    estimated: false,
                    source_note: None,
                })
                .collect(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: crate::models::UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: chrono::Utc::now(),
        };
        let state = crate::service::UsageViewState {
            providers: [(
                provider_id.to_owned(),
                crate::models::ProviderViewState {
                    snapshot: Some(snapshot),
                    source: SnapshotSource::Live,
                    ..crate::models::ProviderViewState::default()
                },
            )]
            .into_iter()
            .collect(),
            last_full_refresh_at: None,
        };
        resolved_provider_metrics(&state, &provider, &catalog_settings, registry)
    }

    #[test]
    fn opencode_defaults_use_the_first_three_tray_metrics() {
        let catalog =
            ProviderRegistry::from_definitions(vec![opencode::definition(), codex::definition()])
                .unwrap();
        let metrics = resolved(
            &catalog,
            "opencode",
            &[("session", 75.0), ("weekly", 80.0), ("monthly", 40.0)],
        );
        let presentation = assemble_taskband_text("OC", &layout(), &metrics, false);
        assert_eq!(presentation.top_line, "OC 75%");
        assert_eq!(presentation.bottom_line, "W 80% · M 40%");
        assert!(presentation.bottom_visible);
    }

    #[test]
    fn codex_without_monthly_omits_the_third_slot() {
        let catalog =
            ProviderRegistry::from_definitions(vec![opencode::definition(), codex::definition()])
                .unwrap();
        let metrics = resolved(&catalog, "codex", &[("session", 25.0), ("weekly", 60.0)]);
        let presentation = assemble_taskband_text("Cx", &layout(), &metrics, false);
        assert_eq!(presentation.top_line, "Cx 25%");
        assert_eq!(presentation.bottom_line, "W 60%");
        assert!(presentation.bottom_visible);
    }

    #[test]
    fn explicit_slots_can_reorder_and_hide_lines() {
        let catalog =
            ProviderRegistry::from_definitions(vec![opencode::definition(), codex::definition()])
                .unwrap();
        let metrics = resolved(
            &catalog,
            "opencode",
            &[("session", 75.0), ("weekly", 80.0), ("monthly", 40.0)],
        );
        let mut layout = layout();
        layout.slot_top = None;
        layout.slot_bottom = Some("opencode.monthly".into());
        layout.slot_bottom_2 = None;
        layout.show_labels = false;
        let presentation = assemble_taskband_text("OC", &layout, &metrics, true);
        assert_eq!(presentation.top_line, "OC");
        assert_eq!(presentation.bottom_line, "40%");
        assert!(presentation.bottom_visible);
    }

    #[test]
    fn no_snapshot_leaves_only_the_short_name() {
        let presentation = assemble_taskband_text("OC", &layout(), &[], false);
        assert_eq!(presentation.top_line, "OC");
        assert_eq!(presentation.bottom_line, "");
        assert!(!presentation.bottom_visible);
    }

    #[test]
    fn explicit_slot_falls_back_to_a_value_metric() {
        let mut layout = layout();
        layout.slot_top = None;
        layout.slot_bottom = Some("opencode.credits".into());
        let metrics = vec![ResolvedTrayMetric {
            id: "opencode.credits".into(),
            short_label: "E".into(),
            value: "$4".into(),
        }];
        let presentation = assemble_taskband_text("OC", &layout, &metrics, true);
        assert_eq!(presentation.top_line, "OC");
        assert_eq!(presentation.bottom_line, "E $4");
    }
}
