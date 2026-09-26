//! macOS 菜单栏集成（tauri-plugin-multiline-menubar）。
//!
//! 职责：与 Windows 的 `taskband` 模块对齐 —— 根据 `AppSettings` +
//! `UsageViewState` 对账每个已启用 provider 在菜单栏上的实例：
//! 创建 / 更新 / 隐藏 / 移除，并监听实例点击事件
//! （左键 → 打开主窗口 popup 并只聚焦该 agent；右键 → 弹出隐藏 / 刷新 /
//! 设置该 agent / 退出的原生上下文菜单）。
//!
//! 与 Windows taskband 的对齐：
//! - 两个平台每个 provider 都是单个实例：插件实例级 LeadingIcon 列图标
//!   （品牌图标）独占左侧一列，右侧上下两行渲染前两个固定指标值。
//! - 插件没有 side/order/padding/margin（macOS 系统自动排列 status item），
//!   因此这些 Windows 专属偏好不生效。
//!
//! 文本组装逻辑为纯函数（可在任意平台单测），所有调用插件的代码
//! 均以 `#[cfg(target_os = "macos")]` 隔离，非 macOS 零影响。

#[cfg(any(target_os = "macos", test))]
use crate::models::AppSettings;
#[cfg(any(target_os = "macos", test))]
use crate::models::{TaskbandColorStyle, TaskbandLayout};
#[cfg(any(target_os = "macos", test))]
use crate::tray_presentation::ResolvedTrayMetric;
#[cfg(target_os = "macos")]
use crate::tray_presentation::{
    pinned_provider_metrics, requested_provider_entries, NativeInstancePlatform,
};
#[cfg(target_os = "macos")]
use crate::{
    desktop_integration::{DesktopIntegration, RuntimeEntryOutcome},
    pacing::NotificationEvaluator,
    providers::{provider_icon_svg, ProviderRegistry},
    service::{ProviderService, UsageViewState},
    settings::SettingsService,
    tray_presentation,
    window::{open_screen, show_main_window, MAIN_WINDOW},
};
#[cfg(target_os = "macos")]
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
#[cfg(target_os = "macos")]
use tauri::{AppHandle, Emitter, EventId, Listener, Manager};
#[cfg(target_os = "macos")]
use tauri_plugin_multiline_menubar::{
    ColorStyle, IconSpec, MenuItemDescriptor, MultilineMenubarExt,
};

/// 与前端 `providerIconPaths.ts` 保持一致的品牌色表。无品牌色的 provider
/// 自动使用系统菜单栏文字色（`Default`）。与 `taskband.rs` 同一张表。
#[cfg(target_os = "macos")]
const BRAND_COLORS: &[(&str, &str)] = &[
    ("antigravity", "#4285F4"),
    ("claude", "#DE7356"),
    ("kimi", "#1783FF"),
    ("minimax", "#E2167E"),
];

#[cfg(target_os = "macos")]
fn brand_color(provider_id: &str) -> Option<&'static str> {
    BRAND_COLORS
        .iter()
        .find(|(id, _)| provider_id.starts_with(id))
        .map(|(_, color)| *color)
}

/// 组装菜单栏实例的两行文本，与 Windows taskband 对齐：最多取前 2 个指标
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

/// 把 provider id 转成可安全拼进 Tauri 事件名的实例 id。
///
/// 插件的 click/menu/remove 事件名是 `multiline-menubar://{id}//...`，而
/// Tauri 事件名只允许字母数字与 `- / : _`；带 account 后缀的 provider id
/// （如 `claude@abc`）含 `@`，必须转义成合法字符，插件实例才会收到事件。
#[cfg(any(target_os = "macos", test))]
fn sanitize_instance_id(provider_id: &str) -> String {
    let mut out = String::with_capacity(provider_id.len());
    for c in provider_id.chars() {
        if c.is_alphanumeric() || matches!(c, '-' | '/' | ':' | '_') {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "provider".to_owned()
    } else {
        out
    }
}

/// 每个已创建 menubar 实例上次应用的配置，用于 diff 避免重复调用 set_*。
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq)]
struct AppliedConfig {
    text: (String, String),
    lines_visible: (bool, bool),
    leading_icon: Option<&'static str>,
    top_color: TaskbandColorStyle,
    bottom_color: TaskbandColorStyle,
    top_bold: bool,
    bottom_bold: bool,
    top_size: f64,
    bottom_size: f64,
    top_align: i32,
    bottom_align: i32,
    tooltip: String,
    visible: bool,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq)]
struct MenubarConfigInput {
    text: (String, String),
    lines_visible: (bool, bool),
    leading_icon: Option<&'static str>,
    tooltip: String,
}

#[cfg(target_os = "macos")]
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq)]
struct DesiredProviderMenubar {
    instance_id: String,
    provider_id: String,
    provider_name: String,
    config: AppliedConfig,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq)]
struct MenubarPlan {
    provider_instances: Vec<DesiredProviderMenubar>,
}

#[cfg(any(target_os = "macos", test))]
fn plan_menubar(provider_instances: Vec<DesiredProviderMenubar>) -> MenubarPlan {
    MenubarPlan { provider_instances }
}

#[cfg(target_os = "macos")]
pub(crate) struct MenubarState {
    created: Mutex<HashMap<String, AppliedConfig>>,
    /// 实例 id -> provider id：点击与右键菜单归属同一个 provider。
    owners: Mutex<HashMap<String, String>>,
    click_listeners: Mutex<HashMap<String, EventId>>,
    menu_listeners: Mutex<HashMap<String, EventId>>,
    remove_listeners: Mutex<HashMap<String, EventId>>,
    /// 已附加的右键菜单签名，语言 / provider 名变化时才重建。
    menu_signatures: Mutex<HashMap<String, String>>,
    /// 是否已关闭插件的自动 popup（由本模块自己打开主窗口）。
    global: Mutex<bool>,
    /// Serializes reconciliation and lets a newer plan supersede a queued one.
    reconcile_generation: AtomicU64,
    reconcile_lock: Mutex<()>,
}

#[cfg(target_os = "macos")]
impl Default for MenubarState {
    fn default() -> Self {
        Self {
            created: Mutex::new(HashMap::new()),
            owners: Mutex::new(HashMap::new()),
            click_listeners: Mutex::new(HashMap::new()),
            menu_listeners: Mutex::new(HashMap::new()),
            remove_listeners: Mutex::new(HashMap::new()),
            menu_signatures: Mutex::new(HashMap::new()),
            global: Mutex::new(false),
            reconcile_generation: AtomicU64::new(0),
            reconcile_lock: Mutex::new(()),
        }
    }
}

#[cfg(target_os = "macos")]
impl MenubarState {
    /// 关闭插件的「左键自动 toggle popup」。Quota01 自己管理主窗口
    /// （与 Windows taskband 一样监听 click 事件再打开），避免与内置
    /// 的 popup / 面板逻辑打架。
    #[cfg(target_os = "macos")]
    fn apply_global(&self, app: &AppHandle) {
        let mut done = self.global.lock().unwrap_or_else(|e| e.into_inner());
        if *done {
            return;
        }
        let _ = app.multiline_menubar().set_auto_popup(false);
        *done = true;
    }

    #[cfg(target_os = "macos")]
    fn apply_instance(
        &self,
        app: &AppHandle,
        id: &str,
        config: AppliedConfig,
    ) -> Result<(), String> {
        let mb = app.multiline_menubar();
        let mut created = self.created.lock().unwrap_or_else(|e| e.into_inner());
        let result = (|| -> Result<(), String> {
            match created.get(id) {
                None => {
                    mb.create(id.to_string())
                        .map_err(|error| error.to_string())?;
                    mb.set_text(id.to_string(), config.text.0.clone(), config.text.1.clone())
                        .map_err(|error| error.to_string())?;
                    mb.set_line_visible(
                        id.to_string(),
                        config.lines_visible.0,
                        config.lines_visible.1,
                    )
                    .map_err(|error| error.to_string())?;
                    mb.set_leading_icon(id.to_string(), to_icon(config.leading_icon))
                        .map_err(|error| error.to_string())?;
                    mb.set_colors(
                        id.to_string(),
                        to_plugin_color(&config.top_color, id),
                        to_plugin_color(&config.bottom_color, ""),
                    )
                    .map_err(|error| error.to_string())?;
                    mb.set_bold(id.to_string(), config.top_bold, config.bottom_bold)
                        .map_err(|error| error.to_string())?;
                    mb.set_font_sizes(id.to_string(), config.top_size, config.bottom_size)
                        .map_err(|error| error.to_string())?;
                    mb.set_alignment(id.to_string(), config.top_align, config.bottom_align)
                        .map_err(|error| error.to_string())?;
                    mb.set_tooltip(id.to_string(), config.tooltip.clone())
                        .map_err(|error| error.to_string())?;
                    mb.set_visible(id.to_string(), config.visible)
                        .map_err(|error| error.to_string())?;
                }
                Some(previous) => {
                    if previous.text != config.text {
                        mb.set_text(id.to_string(), config.text.0.clone(), config.text.1.clone())
                            .map_err(|error| error.to_string())?;
                    }
                    if previous.lines_visible != config.lines_visible {
                        mb.set_line_visible(
                            id.to_string(),
                            config.lines_visible.0,
                            config.lines_visible.1,
                        )
                        .map_err(|error| error.to_string())?;
                    }
                    if previous.leading_icon != config.leading_icon {
                        mb.set_leading_icon(id.to_string(), to_icon(config.leading_icon))
                            .map_err(|error| error.to_string())?;
                    }
                    if previous.top_color != config.top_color
                        || previous.bottom_color != config.bottom_color
                    {
                        mb.set_colors(
                            id.to_string(),
                            to_plugin_color(&config.top_color, id),
                            to_plugin_color(&config.bottom_color, ""),
                        )
                        .map_err(|error| error.to_string())?;
                    }
                    if previous.top_bold != config.top_bold
                        || previous.bottom_bold != config.bottom_bold
                    {
                        mb.set_bold(id.to_string(), config.top_bold, config.bottom_bold)
                            .map_err(|error| error.to_string())?;
                    }
                    if previous.top_size != config.top_size
                        || previous.bottom_size != config.bottom_size
                    {
                        mb.set_font_sizes(id.to_string(), config.top_size, config.bottom_size)
                            .map_err(|error| error.to_string())?;
                    }
                    if previous.top_align != config.top_align
                        || previous.bottom_align != config.bottom_align
                    {
                        mb.set_alignment(id.to_string(), config.top_align, config.bottom_align)
                            .map_err(|error| error.to_string())?;
                    }
                    if previous.tooltip != config.tooltip {
                        mb.set_tooltip(id.to_string(), config.tooltip.clone())
                            .map_err(|error| error.to_string())?;
                    }
                    if previous.visible != config.visible {
                        mb.set_visible(id.to_string(), config.visible)
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
            Ok(())
        })();
        if result.is_ok() {
            created.insert(id.to_string(), config);
        }
        result
    }

    #[cfg(target_os = "macos")]
    fn apply_visible_instance(
        &self,
        apply: impl FnOnce() -> Result<(), String>,
        drain_queued_native_work: impl FnOnce(),
        is_visible: impl FnOnce() -> bool,
        remove: impl FnOnce(),
    ) -> Result<bool, String> {
        match apply() {
            Ok(()) => {
                drain_queued_native_work();
                if is_visible() {
                    Ok(true)
                } else {
                    remove();
                    Ok(false)
                }
            }
            Err(error) => {
                remove();
                Err(error)
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn remove_instance(&self, app: &AppHandle, id: &str) {
        self.remove_instance_with(
            id,
            || {
                let _ = app.multiline_menubar().remove(id.to_owned());
            },
            |listener_id| {
                app.unlisten(listener_id);
            },
        );
    }

    #[cfg(target_os = "macos")]
    fn remove_instance_with(
        &self,
        id: &str,
        remove_from_plugin: impl FnOnce(),
        mut unlisten: impl FnMut(EventId),
    ) {
        let listeners = self.invalidate_instance_state(id);
        for listener_id in listeners {
            unlisten(listener_id);
        }
        remove_from_plugin();
    }

    #[cfg(target_os = "macos")]
    fn invalidate_instance_state(&self, id: &str) -> Vec<EventId> {
        self.created
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.owners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.menu_signatures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);

        [
            self.click_listeners
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(id),
            self.menu_listeners
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(id),
            self.remove_listeners
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(id),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// 为某个实例注册左键点击监听；点击统一归属到其所属 provider
    /// （`provider_id`）：打开主窗口 popup 并让前端只显示该 provider。
    #[cfg(target_os = "macos")]
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
        let event = format!("multiline-menubar://{instance_id}//click");
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
                match menu_bar_click_anchor(&payload) {
                    Some(anchor) => {
                        crate::window::show_main_window_below_menu_bar_item(&window, anchor)
                    }
                    None => crate::window::show_main_window(&window),
                }
            }
            let _ = listener_app.emit("taskband-open", emit_id.clone());
        });
        registered.insert(instance_id.to_string(), listener_id);
    }

    /// 为 Quota01 应用实例注册左键监听：再次点击时切换主窗口。
    /// 为某个实例绑定右键上下文菜单（隐藏 agent / 刷新数据 / 打开该
    /// agent 的设置 / 退出应用），并注册菜单选择监听。菜单文案随语言与
    /// provider 名变化而重建。
    #[cfg(target_os = "macos")]
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
        let (items, signature) = context_menu_items(instance_id, locale, provider_name);
        let mb = app.multiline_menubar();
        if let Err(error) =
            install_context_menu(&self.menu_signatures, instance_id, signature, || {
                mb.set_menu(instance_id.to_string(), items)
                    .map_err(|error| error.to_string())
            })
        {
            crate::app_warn!(
                "menubar",
                "could not install context menu for {instance_id}: {error}"
            );
        }
        self.register_menu_listener(app, instance_id);
        self.register_remove_listener(app, instance_id, provider_id);
    }

    #[cfg(target_os = "macos")]
    fn register_menu_listener(&self, app: &AppHandle, instance_id: &str) {
        let mut registered = self
            .menu_listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if registered.contains_key(instance_id) {
            return;
        }
        let event = format!("multiline-menubar://{instance_id}//menu");
        let listener_app = app.clone();
        let listener_id = app.listen(event, move |event| {
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) else {
                return;
            };
            let Some(instance_id) = payload.get("id").and_then(|v| v.as_str()) else {
                return;
            };
            let Some(item_id) = payload.get("itemId").and_then(|v| v.as_str()) else {
                return;
            };
            // 菜单项 id 形如 `{instance_id}::{action}`：macOS 插件的菜单
            // 事件按进程级 item id 回传，跨实例必须全局唯一（与 taskband
            // 插件的 `MENU_ID_SEPARATOR` 方案一致）。
            let Some(owner) = listener_app
                .state::<MenubarState>()
                .owners
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(instance_id)
                .cloned()
            else {
                return;
            };
            let Some((_, action)) = item_id.rsplit_once("::") else {
                return;
            };
            dispatch_context_menu_action(&listener_app, &owner, action);
        });
        registered.insert(instance_id.to_string(), listener_id);
    }

    /// 用户把 provider 菜单栏实例拖出时，插件会 emit `//remove`。将该
    /// provider 的菜单栏布局持久化为停用，避免下一次对账自动重建实例。
    #[cfg(target_os = "macos")]
    fn register_remove_listener(&self, app: &AppHandle, instance_id: &str, provider_id: &str) {
        let mut registered = self
            .remove_listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if registered.contains_key(instance_id) {
            return;
        }
        let event = format!("multiline-menubar://{instance_id}//remove");
        let instance_id = instance_id.to_owned();
        let provider_id = provider_id.to_owned();
        let listener_instance_id = instance_id.clone();
        let listener_app = app.clone();
        let listener_id = app.listen(event, move |_event| {
            let settings_service = listener_app.state::<Arc<SettingsService>>();
            match settings_service
                .mutate_latest(|settings| disable_provider_layout(settings, &provider_id))
            {
                Ok(updated) => {
                    let provider_service = listener_app.state::<Arc<ProviderService>>();
                    tray_presentation::update(
                        &listener_app,
                        &provider_service.state(),
                        &updated,
                        settings_service.registry(),
                    );
                    let _ = listener_app.emit(
                        "settings-state",
                        crate::commands::settings::settings_view_state(
                            &listener_app,
                            settings_service.inner().as_ref(),
                        ),
                    );
                    crate::app_info!(
                        "menubar",
                        "menu bar instance {listener_instance_id} was removed by the user"
                    );
                }
                Err(error) => {
                    crate::app_warn!(
                        "menubar",
                        "could not persist removal of menu bar instance {listener_instance_id}: {error}"
                    );
                    let menubar = listener_app.state::<MenubarState>();
                    recover_removed_instance(
                        menubar.inner(),
                        &listener_instance_id,
                        || {
                            let _ = listener_app
                                .multiline_menubar()
                                .remove(listener_instance_id.clone());
                        },
                        |listener_id| {
                            listener_app.unlisten(listener_id);
                        },
                        || {
                            let current = settings_service.get();
                            let provider_service = listener_app.state::<Arc<ProviderService>>();
                            tray_presentation::update(
                                &listener_app,
                                &provider_service.state(),
                                &current,
                                settings_service.registry(),
                            );
                            let _ = listener_app.emit(
                                "settings-state",
                                crate::commands::settings::settings_view_state(
                                    &listener_app,
                                    settings_service.inner().as_ref(),
                                ),
                            );
                        },
                    );
                }
            }
        });
        registered.insert(instance_id, listener_id);
    }
}

/// Install a context menu once per signature. Failed installs must remain
/// retryable, so the signature is cached only after the plugin accepts it.
#[cfg(any(target_os = "macos", test))]
fn install_context_menu(
    signatures: &Mutex<HashMap<String, String>>,
    instance_id: &str,
    signature: String,
    install: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    {
        let signatures = signatures.lock().unwrap_or_else(|e| e.into_inner());
        if signatures.get(instance_id).map(String::as_str) == Some(signature.as_str()) {
            return Ok(());
        }
    }

    install()?;
    signatures
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(instance_id.to_owned(), signature);
    Ok(())
}

/// 将品牌色映射到插件 `ColorStyle`：`Default` 在该行无品牌色时保持系统
/// 菜单栏文字色，有品牌色时回退为该品牌色（与 Windows taskband 一致）。
#[cfg(target_os = "macos")]
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

/// 将捆绑的品牌图标 SVG 源转换为插件的 `IconSpec`。该 spec 用于实例级
/// `set_leading_icon`（LeadingIcon 列图标），用 `tint: true` 让单色图标按
/// 模板图语义渲染，跟随系统菜单栏文字色自动适配亮 / 暗模式。
#[cfg(target_os = "macos")]
fn to_icon(svg: Option<&'static str>) -> Option<IconSpec> {
    svg.map(|svg| IconSpec {
        path: None,
        data: Some(svg.to_owned()),
        tint: true,
        size: None,
    })
}

/// 由共有的布局配置构造一个实例的 `AppliedConfig`。
#[cfg(any(target_os = "macos", test))]
fn instance_config(
    input: MenubarConfigInput,
    layout: &TaskbandLayout,
    visible: bool,
) -> AppliedConfig {
    AppliedConfig {
        text: input.text,
        lines_visible: input.lines_visible,
        leading_icon: input.leading_icon,
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
        tooltip: input.tooltip,
        visible,
    }
}

#[cfg(target_os = "macos")]
fn desired_provider_menubars(
    state: &UsageViewState,
    settings: &AppSettings,
    registry: &ProviderRegistry,
) -> Vec<DesiredProviderMenubar> {
    let mut desired = Vec::new();
    for provider_id in requested_provider_entries(settings, registry, NativeInstancePlatform::MacOS)
    {
        let Some(provider) = settings
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
        else {
            continue;
        };
        let Some(definition) = registry.definition(&provider.id) else {
            continue;
        };
        let layout = settings
            .taskband_providers
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        let metrics = pinned_provider_metrics(state, provider, settings, registry);
        let (top_value, bottom_value, bottom_visible) = metric_lines(&metrics);
        let provider_name = settings.provider_display_name(definition).to_owned();
        let config = instance_config(
            MenubarConfigInput {
                text: (top_value, bottom_value),
                lines_visible: (true, bottom_visible),
                leading_icon: provider_icon_svg(&provider.id),
                tooltip: provider_name.clone(),
            },
            &layout,
            true,
        );
        desired.push(DesiredProviderMenubar {
            instance_id: sanitize_instance_id(&provider.id),
            provider_id: provider.id.clone(),
            provider_name,
            config,
        });
    }
    desired
}

/// Applies the shared runtime-entry policy and the macOS-specific UI side
/// effects. Startup panic recovery and normal reconciliation both use this so
/// neither path can leave the process without a usable entry point.
#[cfg(target_os = "macos")]
fn apply_runtime_entry(app: &AppHandle, has_menu_entry: bool) -> RuntimeEntryOutcome {
    let integration = app.state::<DesktopIntegration>();
    if !has_menu_entry {
        integration.set_menu_entry_available(false);
        if let Some(service) = app.try_state::<Arc<ProviderService>>() {
            service.set_native_instance_ids(Vec::new());
        }
        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
            let _ = crate::window::apply_window_mode(
                &window,
                crate::models::WindowMode::Floating,
                true,
            );
            show_main_window(&window);
            open_screen(app, "settings");
        }
        if let Some(settings) = app.try_state::<Arc<SettingsService>>() {
            let _ = app.emit(
                "settings-state",
                crate::commands::settings::settings_view_state(app, settings.inner().as_ref()),
            );
        }
        return RuntimeEntryOutcome::FloatingWindow;
    }
    let recovering_menu_entry = has_menu_entry && !integration.tray_available();
    let floating_window_visible = floating_main_window_visible(app);
    let outcome = integration.ensure_runtime_entry_or_exit(
        has_menu_entry,
        floating_window_visible,
        || {
            app.get_webview_window(MAIN_WINDOW).is_some_and(|window| {
                crate::window::apply_window_mode(&window, crate::models::WindowMode::Floating, true)
                    .is_ok()
            })
        },
        || app.exit(0),
    );
    if recovering_menu_entry {
        let configured_mode = app
            .try_state::<Arc<SettingsService>>()
            .map(|settings| settings.get().window_mode);
        if let Some(configured_mode) = configured_mode {
            let restored = integration.restore_menu_entry_window_mode(configured_mode, |mode| {
                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                    crate::window::apply_window_mode(&window, mode, false)
                        .map(|()| integration.is_floating())
                } else {
                    Ok(integration.apply_window_mode(mode))
                }
            });
            if let Err(error) = restored {
                crate::app_warn!(
                    "menubar",
                    "could not restore the configured window mode after the menu bar entry returned: {error}"
                );
                // Keep the frontend's trayAvailable-derived mode aligned with
                // the visible floating fallback if chrome restoration fails.
                integration.set_menu_entry_available(false);
            }
        }
    }
    outcome
}

#[cfg(target_os = "macos")]
pub(crate) fn recover_runtime_entry_after_panic(app: &AppHandle) -> bool {
    apply_runtime_entry(app, false) != RuntimeEntryOutcome::Exit
}

/// 对账入口：根据设置 + 快照创建 / 更新 / 移除 macOS 菜单栏实例。
/// 挂载点在 `tray_presentation::update()` 内（macOS）。
#[cfg(target_os = "macos")]
pub(crate) fn update(
    app: &AppHandle,
    state: &UsageViewState,
    settings: &AppSettings,
    registry: &ProviderRegistry,
) {
    let menubar = app.state::<MenubarState>();
    menubar.apply_global(app);

    let locale = crate::i18n::resolve(settings.language);
    let plan = plan_menubar(desired_provider_menubars(state, settings, registry));

    let generation = menubar
        .reconcile_generation
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    let reconcile_app = app.clone();
    let spawn = std::thread::Builder::new()
        .name("quota01-menubar-reconcile".to_owned())
        .spawn(move || {
            let menubar = reconcile_app.state::<MenubarState>();
            let _guard = menubar
                .reconcile_lock
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if menubar.reconcile_generation.load(Ordering::SeqCst) != generation {
                return;
            }

            let _ = reconcile_then_publish(
                || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        apply_menubar_plan(&reconcile_app, plan, locale);
                    }));
                    if let Err(payload) = result {
                        crate::app_error!(
                            "menubar",
                            "menubar reconciliation panicked: {payload:?}"
                        );
                        apply_runtime_entry(&reconcile_app, false);
                        Err(())
                    } else {
                        Ok(())
                    }
                },
                || emit_authoritative_settings_state(&reconcile_app),
            );
        });
    if let Err(error) = spawn {
        crate::app_warn!(
            "menubar",
            "could not start menubar reconciliation worker: {error}"
        );
        apply_runtime_entry(app, false);
        emit_authoritative_settings_state(app);
    }
}

#[cfg(any(target_os = "macos", test))]
fn reconcile_then_publish<T, E>(
    reconcile: impl FnOnce() -> Result<T, E>,
    publish: impl FnOnce(),
) -> Result<T, E> {
    let result = reconcile();
    publish();
    result
}

#[cfg(target_os = "macos")]
fn emit_authoritative_settings_state(app: &AppHandle) {
    if let Some(settings) = app.try_state::<Arc<SettingsService>>() {
        let _ = app.emit(
            "settings-state",
            crate::commands::settings::settings_view_state(app, settings.inner().as_ref()),
        );
    }
}

#[cfg(target_os = "macos")]
fn apply_menubar_plan(app: &AppHandle, plan: MenubarPlan, locale: crate::i18n::Locale) {
    let menubar = app.state::<MenubarState>();
    let mut actual_ids = HashSet::new();
    let mut actual_provider_ids = HashSet::new();
    let mut failures = Vec::new();
    for desired in plan.provider_instances {
        let DesiredProviderMenubar {
            instance_id,
            provider_id,
            provider_name,
            config,
        } = desired;
        let visible = menubar.apply_visible_instance(
            || menubar.apply_instance(app, &instance_id, config),
            || {
                // `rect` uses the plugin's synchronous main-thread getter. On
                // the reconciliation worker it drains all queued create/set
                // blocks before visibility is checked.
                let _ = app.multiline_menubar().rect(instance_id.clone());
            },
            || {
                app.multiline_menubar()
                    .is_visible(instance_id.clone())
                    .unwrap_or(false)
            },
            || menubar.remove_instance(app, &instance_id),
        );
        match visible {
            Ok(true) => {
                menubar.register_click_listener(app, &instance_id, &provider_id);
                menubar.register_context_menu(
                    app,
                    &instance_id,
                    &provider_id,
                    &provider_name,
                    locale,
                );
                actual_ids.insert(instance_id);
                actual_provider_ids.insert(provider_id);
            }
            Ok(false) => {
                failures.push(format!("{provider_name}: not visible"));
                crate::app_warn!(
                    "menubar",
                    "menu bar instance {instance_id} was created but is not visible"
                );
            }
            Err(error) => {
                failures.push(format!("{provider_name}: {error}"));
                crate::app_warn!(
                    "menubar",
                    "could not apply menu bar instance {instance_id}: {error}"
                );
            }
        }
    }

    let stale = menubar
        .created
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .keys()
        .filter(|id| !actual_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for id in stale {
        menubar.remove_instance(app, &id);
    }

    app.state::<DesktopIntegration>()
        .set_provider_instance_status(actual_ids.len(), failures);
    if let Some(service) = app.try_state::<Arc<ProviderService>>() {
        service.set_native_instance_ids(actual_provider_ids);
    }
    apply_runtime_entry(app, !actual_ids.is_empty());
}

#[cfg(target_os = "macos")]
const MENU_ACTION_HIDE: &str = "hide";
#[cfg(target_os = "macos")]
const MENU_ACTION_REFRESH: &str = "refresh";
#[cfg(target_os = "macos")]
const MENU_ACTION_SETTINGS: &str = "settings";
#[cfg(target_os = "macos")]
const MENU_ACTION_QUIT: &str = "quit";

/// 组装某个 provider 实例的右键菜单项及其签名。macOS 插件的菜单事件按
/// 进程级 item id 回传（插件内部 `MENU_ITEM_OWNERS` 全局表、后注册者
/// 覆盖先注册者），因此菜单项 id 必须跨实例全局唯一 —— 采用与 Windows
/// taskband 插件相同的 `{instance_id}::{action}` 方案
/// （见 `taskband` 插件的 `MENU_ID_SEPARATOR`）。
#[cfg(target_os = "macos")]
fn context_menu_items(
    instance_id: &str,
    locale: crate::i18n::Locale,
    provider_name: &str,
) -> (Vec<MenuItemDescriptor>, String) {
    let item = |id: String, text: String| MenuItemDescriptor::Item {
        id,
        text,
        accelerator: None,
        enabled: Some(true),
        disabled: None,
    };
    let hide = crate::i18n::bar_action_label(locale, MENU_ACTION_HIDE, provider_name);
    let refresh = crate::i18n::bar_action_label(locale, MENU_ACTION_REFRESH, provider_name);
    let settings = crate::i18n::bar_action_label(locale, MENU_ACTION_SETTINGS, provider_name);
    let quit = crate::i18n::bar_action_label(locale, MENU_ACTION_QUIT, provider_name);
    let action_id = |action: &str| format!("{instance_id}::{action}");
    let items = vec![
        item(action_id(MENU_ACTION_HIDE), hide.clone()),
        item(action_id(MENU_ACTION_REFRESH), refresh.clone()),
        item(action_id(MENU_ACTION_SETTINGS), settings.clone()),
        MenuItemDescriptor::Separator,
        item(action_id(MENU_ACTION_QUIT), quit.clone()),
    ];
    // 语言或 provider 名变化都会反映在文案里，因此用文案做签名即可。
    let signature = format!("{hide}\u{1}{refresh}\u{1}{settings}\u{1}{quit}");
    (items, signature)
}

/// 从插件 click 事件载荷中提取被点击实例的屏幕矩形（AppKit points：
/// 左下原点、y 向上），用于把 popup 锚定到该实例正下方。旧版插件载荷缺
/// 字段时返回 `None`，调用方回退到默认（托盘居中）定位。
#[cfg(target_os = "macos")]
fn menu_bar_click_anchor(payload: &serde_json::Value) -> Option<crate::window::MenuBarAnchor> {
    let rect = payload.get("rect")?;
    Some(crate::window::MenuBarAnchor {
        rect_x: rect.get("x")?.as_f64()?,
        rect_y: rect.get("y")?.as_f64()?,
        rect_width: rect.get("width")?.as_f64()?,
        rect_height: rect.get("height")?.as_f64()?,
    })
}

/// 分发右键菜单选择到对应动作。
#[cfg(target_os = "macos")]
fn dispatch_context_menu_action(app: &AppHandle, provider_id: &str, action: &str) {
    match action {
        MENU_ACTION_HIDE => hide_agent(app, provider_id),
        MENU_ACTION_REFRESH => refresh_agent(app, provider_id),
        MENU_ACTION_SETTINGS => open_provider_settings(app, provider_id),
        MENU_ACTION_QUIT => quit_application(app),
        _ => crate::app_warn!("menubar", "ignored context menu action {action}"),
    }
}

/// 延迟退出应用，避开右键菜单 tracking loop 未结束时同步 `app.exit`
/// 造成的卡死（插件 v1.6.1 修复的同一问题）。
#[cfg(target_os = "macos")]
fn recover_removed_instance(
    menubar: &MenubarState,
    instance_id: &str,
    remove_from_plugin: impl FnOnce(),
    unlisten: impl FnMut(EventId),
    reconcile: impl FnOnce(),
) {
    menubar.remove_instance_with(instance_id, remove_from_plugin, unlisten);
    reconcile();
}

#[cfg(target_os = "macos")]
fn floating_main_window_visible(app: &AppHandle) -> bool {
    app.state::<DesktopIntegration>().is_floating()
        && app.get_webview_window(MAIN_WINDOW).is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
        })
}

#[cfg(target_os = "macos")]
fn quit_application(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        crate::window::finish_native_panel_resize(&window);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        app.exit(0);
    });
}

/// 「隐藏这个 agent」：与主窗口里 Hide provider 一致，把该 provider 设为
/// 未启用，随后对账会移除它的全部菜单栏实例与右键菜单。
/// 与 `taskband.rs::hide_agent` 逻辑一致。
#[cfg(target_os = "macos")]
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
    disable_provider_layout(&mut next, provider_id);
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
                "menubar",
                "hid menu bar instance for {provider_id} from its context menu"
            );
        }
        Err(error) => crate::app_warn!(
            "menubar",
            "could not hide agent {provider_id} from its context menu: {error}"
        ),
    }
}

#[cfg(any(target_os = "macos", test))]
fn disable_provider_layout(settings: &mut AppSettings, provider_id: &str) {
    settings
        .taskband_providers
        .entry(provider_id.to_owned())
        .or_default()
        .enabled = false;
}

/// 「刷新数据」：强制刷新该 provider 并更新托盘 / 菜单栏展示。
#[cfg(target_os = "macos")]
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
/// （前端 `provider:{provider_id}` 屏幕）。
#[cfg(target_os = "macos")]
fn open_provider_settings(app: &AppHandle, provider_id: &str) {
    open_screen(app, &format!("provider:{provider_id}"));
}

#[cfg(test)]
mod tests {
    use super::{
        disable_provider_layout, plan_menubar, reconcile_then_publish, DesiredProviderMenubar,
    };
    use crate::models::{AppSettings, ProviderLayout, TaskbandLayout};

    #[test]
    fn hiding_instance_disables_its_layout_without_disabling_provider() {
        let mut settings = AppSettings::default();
        settings.providers.push(ProviderLayout {
            id: "codex".into(),
            enabled: true,
            detected: false,
            expanded: false,
            #[cfg(not(target_os = "macos"))]
            keychain_access_granted: false,
            metrics: Vec::new(),
        });
        settings
            .taskband_providers
            .insert("codex".into(), TaskbandLayout::default());

        disable_provider_layout(&mut settings, "codex");

        assert!(settings.providers[0].enabled);
        assert!(!settings.taskband_providers["codex"].enabled);
    }

    #[test]
    fn no_requested_provider_instances_produces_an_empty_menu_plan() {
        let plan = plan_menubar(Vec::<DesiredProviderMenubar>::new());
        assert!(plan.provider_instances.is_empty());
    }

    #[test]
    fn settings_state_is_published_after_reconciliation_even_when_it_fails() {
        let mut published = false;
        let result =
            reconcile_then_publish(|| Err::<(), _>("partial failure"), || published = true);
        assert_eq!(result, Err("partial failure"));
        assert!(published);
    }
}
