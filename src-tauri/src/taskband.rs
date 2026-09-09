//! Windows 任务栏 taskband 集成（tauri-plugin-multiline-taskband）。
//!
//! 职责：根据 `AppSettings` + `UsageViewState` 对账每个已启用监控在任务栏上
//! 的两行文本标签实例 —— 创建 / 更新 / 隐藏 / 移除，并监听实例点击事件
//! （左键 → 在点击实例上方打开主窗口 popup 并只聚焦该 agent；右键 →
//! 显示隐藏 / 刷新 / 设置该 agent / 退出的原生上下文菜单）。
//!
//! 文本组装逻辑为纯函数（可在任意平台单测），所有调用插件的代码
//! 均以 `#[cfg(target_os = "windows")]` 隔离，非 Windows 零影响。

#[cfg(target_os = "windows")]
use crate::models::{
    AppSettings, TaskbandColorStyle, TaskbandLayout, TaskbandPreferences, TaskbandSide,
};
#[cfg(target_os = "windows")]
use crate::pacing::NotificationEvaluator;
#[cfg(target_os = "windows")]
use crate::providers::ProviderRegistry;
#[cfg(target_os = "windows")]
use crate::service::{ProviderService, UsageViewState};
#[cfg(target_os = "windows")]
use crate::settings::SettingsService;
#[cfg(target_os = "windows")]
use crate::tray_presentation::pinned_provider_metrics;
use crate::tray_presentation::ResolvedTrayMetric;
#[cfg(target_os = "windows")]
use crate::window::{open_screen, TaskbandAnchor, MAIN_WINDOW};
#[cfg(target_os = "windows")]
use std::collections::{HashMap, HashSet};
#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use tauri::{AppHandle, Emitter, EventId, Listener, Manager};

#[cfg(target_os = "windows")]
use tauri_plugin_multiline_taskband::{
    ColorStyle, IconSpec, MenuItemDescriptor, MultilineTaskbandExt, Side,
};

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
    /// 实例 id -> provider id：logo 与指标实例归属同一个 provider。
    owners: Mutex<HashMap<String, String>>,
    click_listeners: Mutex<HashMap<String, EventId>>,
    menu_listeners: Mutex<HashMap<String, EventId>>,
    /// 已附加的右键菜单签名，语言 / provider 名变化时才重建。
    menu_signatures: Mutex<HashMap<String, String>>,
    global: Mutex<Option<GlobalConfig>>,
}

#[cfg(target_os = "windows")]
impl Default for TaskbandState {
    fn default() -> Self {
        Self {
            created: Mutex::new(HashMap::new()),
            owners: Mutex::new(HashMap::new()),
            click_listeners: Mutex::new(HashMap::new()),
            menu_listeners: Mutex::new(HashMap::new()),
            menu_signatures: Mutex::new(HashMap::new()),
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
        let _ = app.multiline_taskband().set_menu(id.to_string(), None);
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
        if let Some(listener_id) = self
            .menu_listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
        {
            app.unlisten(listener_id);
        }
        self.owners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.menu_signatures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
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
    /// （`provider_id`）：在被点击实例上方打开主窗口 popup（横向跟随鼠标），
    /// 并让前端只显示该 provider。
    fn register_click_listener(&self, app: &AppHandle, instance_id: &str, provider_id: &str) {
        self.owners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(instance_id.to_string(), provider_id.to_string());
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
            let anchor = taskband_click_anchor(&payload);
            if let Some(window) = listener_app.get_webview_window(MAIN_WINDOW) {
                match anchor {
                    Some(anchor) => crate::window::show_main_window_anchored(&window, anchor),
                    None => crate::window::show_main_window(&window),
                }
            }
            let _ = listener_app.emit("taskband-open", emit_id.clone());
        });
        registered.insert(instance_id.to_string(), listener_id);
    }

    /// 为某个实例绑定右键上下文菜单（隐藏 agent / 刷新数据 / 打开该
    /// agent 的设置 / 退出应用），并注册菜单选择监听。菜单文案随语言与
    /// provider 名变化而重建。
    fn register_context_menu(
        &self,
        app: &AppHandle,
        instance_id: &str,
        provider_id: &str,
        provider_name: &str,
        locale: crate::i18n::Locale,
    ) {
        self.owners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(instance_id.to_string(), provider_id.to_string());
        let (items, signature) = context_menu_items(locale, provider_name);
        let tb = app.multiline_taskband();
        let mut signatures = self
            .menu_signatures
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if signatures.get(instance_id).map(String::as_str) != Some(signature.as_str()) {
            let _ = tb.set_menu(instance_id.to_string(), Some(items));
            signatures.insert(instance_id.to_string(), signature);
        }
        drop(signatures);
        self.register_menu_listener(app, instance_id);
    }

    fn register_menu_listener(&self, app: &AppHandle, instance_id: &str) {
        let mut registered = self
            .menu_listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if registered.contains_key(instance_id) {
            return;
        }
        let event = format!("multiline-taskband://{instance_id}//menu");
        let listener_app = app.clone();
        let listener_id = app.listen(event, move |event| {
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) else {
                return;
            };
            let Some(instance_id) = payload.get("id").and_then(|v| v.as_str()) else {
                return;
            };
            let Some(action) = payload.get("itemId").and_then(|v| v.as_str()) else {
                return;
            };
            let Some(provider_id) = listener_app
                .state::<TaskbandState>()
                .owners
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(instance_id)
                .cloned()
            else {
                return;
            };
            dispatch_context_menu_action(&listener_app, &provider_id, action);
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

    let locale = crate::i18n::resolve(settings.language);
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
        // 与 mac menubar 一致：只展示用户固定的（pinned）指标。
        let metrics = pinned_provider_metrics(state, provider, settings, registry);
        if metrics.is_empty() {
            continue;
        }
        let icon_svg = crate::providers::provider_icon_svg(&provider.id);
        let provider_name = registry
            .definition(&provider.id)
            .map(|definition| settings.provider_display_name(definition))
            .unwrap_or(&provider.id)
            .to_owned();
        let base_order = (index as u64) * 2;
        // 插件同侧实例按 order 升序从边缘向里排列：Left 从 Start 向右排，
        // Right 从托盘向左排。为保证任何一侧都呈现「图标在左、数据在右」，
        // 右边缘需把指标实例放在更靠近边缘的位置（order 更小）。
        let (logo_order, metrics_order) = match side {
            TaskbandSide::Left => (base_order, base_order + 1),
            TaskbandSide::Right => (base_order + 1, base_order),
        };
        let (top_value, bottom_value, bottom_visible) = metric_lines(&metrics);

        // 仅图标 logo 实例：顶部行渲染品牌图标，底部行隐藏。
        // 注意：实例 id 会拼进 clicked 事件名，Tauri 事件名不允许 "."，
        // 因此后缀用 "-logo"（连字符合法）而非 ".logo"。
        if let Some(svg) = icon_svg {
            let logo_id = format!("{}-logo", provider.id);
            let logo_config = instance_config(
                side,
                logo_order,
                (String::new(), String::new()),
                (true, false),
                Some(svg),
                &layout,
                true,
            );
            taskband.apply_instance(app, &logo_id, logo_config);
            taskband.register_click_listener(app, &logo_id, &provider.id);
            taskband.register_context_menu(app, &logo_id, &provider.id, &provider_name, locale);
            desired_ids.insert(logo_id);
        }

        // 指标实例：最多显示前 2 个指标值（对齐 mac menubar）。
        let metrics_id = provider.id.clone();
        let metrics_config = instance_config(
            side,
            metrics_order,
            (top_value, bottom_value),
            (true, bottom_visible),
            None,
            &layout,
            true,
        );
        taskband.apply_instance(app, &metrics_id, metrics_config);
        taskband.register_click_listener(app, &metrics_id, &provider.id);
        taskband.register_context_menu(app, &metrics_id, &provider.id, &provider_name, locale);
        desired_ids.insert(metrics_id);
    }

    let stale = taskband
        .created
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .keys()
        .filter(|id| !desired_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for id in stale {
        taskband.remove_instance(app, &id);
    }
}

#[cfg(target_os = "windows")]
const MENU_ACTION_HIDE: &str = "hide";
#[cfg(target_os = "windows")]
const MENU_ACTION_REFRESH: &str = "refresh";
#[cfg(target_os = "windows")]
const MENU_ACTION_SETTINGS: &str = "settings";
#[cfg(target_os = "windows")]
const MENU_ACTION_QUIT: &str = "quit";

/// 组装某个 provider 实例的右键菜单项及其签名。菜单项 id 由插件拼上实例 id
/// 前缀后回传，因此这里只需在单个菜单内唯一即可。
#[cfg(target_os = "windows")]
fn context_menu_items(
    locale: crate::i18n::Locale,
    provider_name: &str,
) -> (Vec<MenuItemDescriptor>, String) {
    let item = |id: &'static str, text: String| MenuItemDescriptor::Item {
        id: id.to_owned(),
        text,
        accelerator: None,
        enabled: Some(true),
    };
    let items = vec![
        item(
            MENU_ACTION_HIDE,
            crate::i18n::taskband_action_label(locale, MENU_ACTION_HIDE, provider_name),
        ),
        item(
            MENU_ACTION_REFRESH,
            crate::i18n::taskband_action_label(locale, MENU_ACTION_REFRESH, provider_name),
        ),
        item(
            MENU_ACTION_SETTINGS,
            crate::i18n::taskband_action_label(locale, MENU_ACTION_SETTINGS, provider_name),
        ),
        MenuItemDescriptor::Separator,
        item(
            MENU_ACTION_QUIT,
            crate::i18n::taskband_action_label(locale, MENU_ACTION_QUIT, provider_name),
        ),
    ];
    let locale_code = match locale {
        crate::i18n::Locale::En => "en",
        crate::i18n::Locale::ZhCn => "zh-CN",
    };
    let signature = format!(
        "locale:{locale_code}\u{1}\u{1}hide:{provider_name}\u{1}refresh:{provider_name}\u{1}settings:{provider_name}\u{1}quit"
    );
    (items, signature)
}

/// 从插件 click 事件载荷中提取「点击点 + 实例屏幕矩形」（物理像素），用于把
/// popup 锚定到被点击实例上方。旧版插件载荷缺字段时返回 `None`，调用方回退
/// 到默认（托盘居中）定位。
#[cfg(target_os = "windows")]
fn taskband_click_anchor(payload: &serde_json::Value) -> Option<TaskbandAnchor> {
    let position = payload.get("position")?;
    let rect = payload.get("rect")?;
    Some(TaskbandAnchor {
        click_x: position.get("x")?.as_f64()?,
        rect_x: rect.get("x")?.as_f64()?,
        rect_y: rect.get("y")?.as_f64()?,
        rect_width: rect.get("width")?.as_f64()?,
        rect_height: rect.get("height")?.as_f64()?,
    })
}

/// 分发右键菜单选择到对应动作。
#[cfg(target_os = "windows")]
fn dispatch_context_menu_action(app: &AppHandle, provider_id: &str, action: &str) {
    match action {
        MENU_ACTION_HIDE => hide_agent(app, provider_id),
        MENU_ACTION_REFRESH => refresh_agent(app, provider_id),
        MENU_ACTION_SETTINGS => open_provider_settings(app, provider_id),
        MENU_ACTION_QUIT => quit_from_taskband(app),
        _ => crate::app_warn!("taskband", "ignored context menu action {action}"),
    }
}

/// 「隐藏这个 agent」：与主窗口里 Hide provider 一致，把该 provider 设为
/// 未启用，随后对账会移除它的全部 taskband 实例与右键菜单。
#[cfg(target_os = "windows")]
fn hide_agent(app: &AppHandle, provider_id: &str) {
    let settings_service = app.state::<Arc<SettingsService>>();
    let service = app.state::<Arc<ProviderService>>();
    let current = settings_service.get();
    if !current
        .providers
        .iter()
        .any(|provider| provider.id == provider_id && provider.enabled)
    {
        return;
    }
    let mut next = current.clone();
    for provider in &mut next.providers {
        if provider.id == provider_id {
            provider.enabled = false;
            break;
        }
    }
    let expected_settings = settings_service.settings_revision();
    let expected_account = settings_service.account_revision();
    match settings_service.update_from_view(next, expected_settings, expected_account) {
        Ok(updated) => {
            crate::tray_presentation::update(
                app,
                &service.state(),
                &updated,
                settings_service.registry(),
            );
            let _ = app.emit(
                "settings-state",
                crate::commands::settings::settings_view_state(
                    app,
                    settings_service.inner().as_ref(),
                ),
            );
            crate::app_info!(
                "taskband",
                "hidden agent {provider_id} from its context menu"
            );
        }
        Err(error) => crate::app_warn!(
            "taskband",
            "could not hide agent {provider_id} from its context menu: {error}"
        ),
    }
}

/// 「刷新数据」：强制刷新该 provider 并更新托盘 / taskband 展示。
#[cfg(target_os = "windows")]
fn refresh_agent(app: &AppHandle, provider_id: &str) {
    let service = app.state::<Arc<ProviderService>>().inner().clone();
    let settings_service = app.state::<Arc<SettingsService>>().inner().clone();
    let notifications = app.state::<Arc<NotificationEvaluator>>().inner().clone();
    let task_app = app.clone();
    let provider_id = provider_id.to_owned();
    tauri::async_runtime::spawn(async move {
        if !settings_service
            .enabled_provider_ids()
            .iter()
            .any(|id| id == &provider_id)
        {
            return;
        }
        service.refresh(&provider_id, true).await;
        let state = service.state();
        let _ = task_app.emit("usage-state", &state);
        crate::notifications::finish_refresh(&task_app, &state, &settings_service, &notifications);
    });
}

/// 「设置这个 agent」：打开主窗口并直接进入该 provider 的设置页
/// （前端 `provider:{provider_id}` 屏幕，即 Customize 里的单个 provider
/// 详情，含该 provider 的任务栏 / 指标等配置）。
#[cfg(target_os = "windows")]
fn open_provider_settings(app: &AppHandle, provider_id: &str) {
    open_screen(app, &format!("provider:{provider_id}"));
}

/// 「退出应用」：结束 native 面板拖拽状态后退出进程。
#[cfg(target_os = "windows")]
fn quit_from_taskband(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        crate::window::finish_native_panel_resize(&window);
    }
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::{
        models::{QuotaFormat, QuotaWindow, SnapshotSource},
        providers::{codex, opencode, ProviderRegistry},
        settings::default_settings,
        tray_presentation::{pinned_provider_metrics, ResolvedTrayMetric},
    };

    use super::metric_lines;

    fn metric(id: &str, value: &str) -> ResolvedTrayMetric {
        ResolvedTrayMetric {
            id: id.into(),
            short_label: String::new(),
            value: value.into(),
        }
    }

    /// 用真实 provider 定义解析指标，验证 taskband 与 mac menubar 使用同一套
    /// pinned 选择规则。`pin_first_two` 模拟用户固定了前两个 quota 指标。
    fn opencode_metrics(pin_first_two: bool) -> Vec<ResolvedTrayMetric> {
        let catalog =
            ProviderRegistry::from_definitions(vec![opencode::definition(), codex::definition()])
                .unwrap();
        let mut catalog_settings =
            default_settings(&catalog, &HashSet::from(["opencode".to_owned()]));
        catalog_settings.usage_display = crate::models::UsageDisplay::Used;
        if pin_first_two {
            for item in &mut catalog_settings.providers {
                if item.id == "opencode" {
                    for (index, metric) in item.metrics.iter_mut().enumerate() {
                        metric.pinned = index < 2;
                    }
                }
            }
        }
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
        pinned_provider_metrics(&state, &provider, &catalog_settings, &catalog)
    }

    fn resolved_opencode() -> Vec<ResolvedTrayMetric> {
        opencode_metrics(true)
    }

    #[test]
    fn first_two_metrics_are_displayed() {
        let (top, bottom, bottom_visible) = metric_lines(&resolved_opencode());
        assert_eq!(top, "75%");
        assert_eq!(bottom, "80%");
        assert!(bottom_visible);
    }

    #[test]
    fn unpinned_metrics_stay_hidden_like_macos() {
        assert!(opencode_metrics(false).is_empty());
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
