//! Windows 任务栏 taskband 集成（tauri-plugin-multiline-taskband）。
//!
//! 职责：根据 `AppSettings` + `UsageViewState` 对账每个已启用监控在任务栏上
//! 的两行文本标签实例 —— 创建 / 更新 / 隐藏 / 移除，并监听实例点击事件
//! （左键 → 打开主窗口 popup 并滚动到对应监控）。
//!
//! 文本组装逻辑为纯函数（可在任意平台单测），所有调用插件的代码
//! 均以 `#[cfg(target_os = "windows")]` 隔离，非 Windows 零影响。

#[cfg(target_os = "windows")]
use crate::models::{AppSettings, TaskbandColorStyle, TaskbandLayout, TaskbandPreferences, TaskbandSide};
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
use tauri_plugin_multiline_taskband::{ColorStyle, IconSpec, MultilineTaskbandExt, Side};

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

/// 组装指标实例的两行任务栏文本，与 mac menubar 对齐：最多取前 2 个指标
/// 值（无标签前缀）。返回 `（第一行值，第二行值，是否显示第二行）`。
pub(crate) fn metric_lines(metrics: &[ResolvedTrayMetric]) -> (String, String, bool) {
    let top = metrics
        .first()
        .map(|metric| metric.value.clone())
        .unwrap_or_default();
    let bottom = metrics
        .get(1)
        .map(|metric| metric.value.clone())
        .unwrap_or_default();
    (top, bottom, metrics.get(1).is_some())
}

/// 每个已创建 taskband 实例上次应用的配置，用于 diff 避免重复调用 set_*。
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq)]
struct AppliedConfig {
    side: TaskbandSide,
    order: u64,
    text: (String, String),
    lines_visible: (bool, bool),
    top_icon: Option<&'static str>,
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
                let _ = tb.set_icon(id.to_string(), to_icon(config.top_icon), None);
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
                if previous.top_icon != config.top_icon {
                    let _ = tb.set_icon(id.to_string(), to_icon(config.top_icon), None);
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

    /// 为某个实例注册左键点击监听；点击统一归属到其所属 provider
    /// （`provider_id`），用于打开主窗口并滚动到对应监控。
    fn register_click_listener(&self, app: &AppHandle, instance_id: &str, provider_id: &str) {
        let mut registered = self
            .click_listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if registered.contains_key(instance_id) {
            return;
        }
        let event = format!("multiline-taskband://{instance_id}//click");
        let emit_id = provider_id.to_string();
        let listener_app = app.clone();
        let listener_id = app.listen(event, move |event| {
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) else {
                return;
            };
            if payload.get("button").and_then(|b| b.as_str()) != Some("left") {
                return;
            }
            if let Some(window) = listener_app.get_webview_window(MAIN_WINDOW) {
                crate::window::show_main_window(&window);
            }
            let _ = listener_app.emit("taskband-open", emit_id.clone());
        });
        registered.insert(instance_id.to_string(), listener_id);
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

/// 将捆绑的品牌图标 SVG 源转换为插件的 `IconSpec`。用 `tint: true` 让图标
/// 跟随该行 `set_colors` 的上色（缺省为品牌色 / 系统任务栏文字色）。
#[cfg(target_os = "windows")]
fn to_icon(svg: Option<&'static str>) -> Option<IconSpec> {
    svg.map(|svg| IconSpec {
        path: None,
        data: Some(svg.to_owned()),
        tint: true,
    })
}

/// 由共有的布局配置构造一个实例的 `AppliedConfig`。
#[cfg(target_os = "windows")]
fn instance_config(
    side: TaskbandSide,
    order: u64,
    text: (String, String),
    lines_visible: (bool, bool),
    top_icon: Option<&'static str>,
    layout: &TaskbandLayout,
    visible: bool,
) -> AppliedConfig {
    AppliedConfig {
        side,
        order,
        text,
        lines_visible,
        top_icon,
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
        visible,
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
        if registry.definition(&provider.id).is_none() {
            continue;
        }
        let layout = settings
            .taskband_providers
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        if !layout.enabled {
            continue;
        }
        let side = layout.side.unwrap_or(prefs.default_side);
        let metrics = resolved_provider_metrics(state, provider, settings, registry);
        let icon_svg = crate::providers::provider_icon_svg(&provider.id);
        let has_snapshot = state
            .providers
            .get(&provider.id)
            .and_then(|p| p.snapshot.as_ref())
            .is_some();
        let base_order = (index as u64) * 2;
        let (top_value, bottom_value, bottom_visible) = metric_lines(&metrics);

        // 仅图标 logo 实例：顶部行渲染品牌图标，底部行隐藏。
        if let Some(svg) = icon_svg {
            let logo_id = format!("{}.logo", provider.id);
            let logo_config = instance_config(
                side,
                base_order,
                (String::new(), String::new()),
                (true, false),
                Some(svg),
                &layout,
                has_snapshot,
            );
            taskband.apply_instance(app, &logo_id, logo_config);
            taskband.register_click_listener(app, &logo_id, &provider.id);
            desired_ids.insert(logo_id);
        }

        // 指标实例：最多显示前 2 个指标值（对齐 mac menubar）。
        let metrics_id = provider.id.clone();
        let metrics_config = instance_config(
            side,
            base_order + 1,
            (top_value, bottom_value),
            (true, bottom_visible),
            None,
            &layout,
            has_snapshot,
        );
        taskband.apply_instance(app, &metrics_id, metrics_config);
        taskband.register_click_listener(app, &metrics_id, &provider.id);
        desired_ids.insert(metrics_id);
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
        tray_presentation::{resolved_provider_metrics, ResolvedTrayMetric},
    };

    use super::metric_lines;

    fn metric(id: &str, value: &str) -> ResolvedTrayMetric {
        ResolvedTrayMetric {
            id: id.into(),
            short_label: String::new(),
            value: value.into(),
        }
    }

    /// 用真实 provider 定义解析全部启用指标，验证「自动取前 2 个」的能力。
    fn resolved_opencode() -> Vec<ResolvedTrayMetric> {
        let catalog = ProviderRegistry::from_definitions(vec![
            opencode::definition(),
            codex::definition(),
        ])
        .unwrap();
        let mut catalog_settings =
            default_settings(&catalog, &HashSet::from(["opencode".to_owned()]));
        catalog_settings.usage_display = crate::models::UsageDisplay::Used;
        let provider = catalog_settings
            .providers
            .iter()
            .find(|p| p.id == "opencode")
            .unwrap()
            .clone();
        let snapshot = crate::models::ProviderSnapshot {
            provider_id: "opencode".into(),
            plan: None,
            quotas: [("session", 75.0), ("weekly", 80.0), ("monthly", 40.0)]
                .into_iter()
                .map(|(id, percent)| QuotaWindow {
                    id: id.into(),
                    label: id.into(),
                    used_percent: percent,
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
                "opencode".to_owned(),
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
        resolved_provider_metrics(&state, &provider, &catalog_settings, &catalog)
    }

    #[test]
    fn first_two_metrics_are_displayed() {
        let (top, bottom, bottom_visible) = metric_lines(&resolved_opencode());
        assert_eq!(top, "75%");
        assert_eq!(bottom, "80%");
        assert!(bottom_visible);
    }

    #[test]
    fn metric_lines_uses_the_first_two_values() {
        let metrics = vec![
            metric("session", "75%"),
            metric("weekly", "80%"),
            metric("monthly", "40%"),
        ];
        let (top, bottom, bottom_visible) = metric_lines(&metrics);
        assert_eq!(top, "75%");
        assert_eq!(bottom, "80%");
        assert!(bottom_visible);
    }

    #[test]
    fn metric_lines_hides_bottom_with_a_single_metric() {
        let metrics = vec![metric("session", "75%")];
        let (top, bottom, bottom_visible) = metric_lines(&metrics);
        assert_eq!(top, "75%");
        assert_eq!(bottom, "");
        assert!(!bottom_visible);
    }

    #[test]
    fn metric_lines_is_empty_without_metrics() {
        let (top, bottom, bottom_visible) = metric_lines(&[]);
        assert_eq!(top, "");
        assert_eq!(bottom, "");
        assert!(!bottom_visible);
    }
}
