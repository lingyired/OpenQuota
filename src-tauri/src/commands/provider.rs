use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WindowEvent};
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

use crate::{
    commands::settings::settings_view_state,
    models::{ApiKeyMutationOutcome, ApiKeyStatus, ProviderApiKeyState, ProviderLink},
    notifications::finish_refresh,
    pacing::NotificationEvaluator,
    providers::{ProviderRegistry, UsageProvider, WebviewAuth, WebviewCredentialSource},
    service::ProviderService,
    settings::SettingsService,
    tray_presentation,
};

const PROVIDER_SESSION_WINDOW_CLOSED_EVENT: &str = "provider-session-window-closed";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSessionWindowClosedEvent {
    provider_id: String,
}

#[derive(Default)]
pub struct ProviderSessionCloseGuard {
    labels: Mutex<HashSet<String>>,
    /// 正在抓取凭据的 provider。并发登录窗口各自触发一次抓取时，只有第一次会真正
    /// 执行——日志里两条 `storage diagnostic` 落在同一毫秒（`.207` / `.208`），就是
    /// 重复建窗与前端自动 capture 叠加出的并发抓取。重复的那次直接让出，避免同时
    /// 读写同一个会话、也避免后到的失败覆盖掉先到的成功。
    capturing: Mutex<HashSet<String>>,
    /// 被拦下的关闭请求：窗口 label → 拦下的时刻。
    ///
    /// 拦截的意义是「先抓取、抓完再关」，但抓取要靠前端监听
    /// `provider-session-window-closed` 才会发生——而承载监听的
    /// `ProviderSessionActions` 只在卡片处于错误态时才渲染。若用户从别处打开了登录
    /// 窗口而没人监听，拦截就再也不会被解开，窗口变成一个关不掉的窗口。这里记住
    /// 拦下的时刻，超过宽限期仍无人接手就放行，任何情况下都不会把窗口锁死。
    held_closes: Mutex<HashMap<String, Instant>>,
}

impl ProviderSessionCloseGuard {
    fn mark(&self, window_label: &str) {
        if let Ok(mut labels) = self.labels.lock() {
            labels.insert(window_label.to_owned());
        }
    }

    fn unmark(&self, window_label: &str) {
        if let Ok(mut labels) = self.labels.lock() {
            labels.remove(window_label);
        }
    }

    fn consume(&self, window_label: &str) -> bool {
        self.labels
            .lock()
            .map(|mut labels| labels.remove(window_label))
            .unwrap_or(false)
    }

    fn is_marked(&self, window_label: &str) -> bool {
        self.labels
            .lock()
            .map(|labels| labels.contains(window_label))
            .unwrap_or(false)
    }

    /// 记下这次被拦下的关闭请求，返回它是否为**首次**拦下。
    ///
    /// 只有首次拦下需要通知前端去抓取；再次拦下说明用户已经在反复点关闭了。
    fn hold_close(&self, window_label: &str, now: Instant) -> bool {
        let Ok(mut held) = self.held_closes.lock() else {
            return false;
        };
        held.insert(window_label.to_owned(), now).is_none()
    }

    /// 这次关闭请求是否已经等过了宽限期，该放行了。
    fn close_hold_expired(&self, window_label: &str, now: Instant, grace: Duration) -> bool {
        let Ok(held) = self.held_closes.lock() else {
            return false;
        };
        held.get(window_label)
            .is_some_and(|held_since| now.duration_since(*held_since) >= grace)
    }

    fn release_close(&self, window_label: &str) {
        if let Ok(mut held) = self.held_closes.lock() {
            held.remove(window_label);
        }
    }

    /// 尝试取得 `provider_id` 的抓取权；已有一次抓取在跑时返回 `None`。
    fn begin_capture(&self, provider_id: &str) -> Option<CaptureGuard<'_>> {
        let mut capturing = self.capturing.lock().ok()?;
        if !capturing.insert(provider_id.to_owned()) {
            return None;
        }
        Some(CaptureGuard {
            owner: self,
            provider_id: provider_id.to_owned(),
        })
    }
}

/// 抓取权的 RAII 句柄：无论正常返回还是提前报错，都会把 provider 从「正在抓取」
/// 集合里摘掉，不会因为某条错误路径把 provider 永久锁死。
pub struct CaptureGuard<'a> {
    owner: &'a ProviderSessionCloseGuard,
    provider_id: String,
}

impl Drop for CaptureGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut capturing) = self.owner.capturing.lock() {
            capturing.remove(&self.provider_id);
        }
    }
}

fn read_cookie_session(
    window: &tauri::WebviewWindow,
    cookie_name: &str,
) -> Result<Zeroizing<String>, String> {
    // `cookies_for_url` compares the cookie domain exactly on macOS, which misses
    // valid parent-domain cookies such as `.trae.cn`; this window is dedicated to
    // one provider, so enumerate its cookies and match the declared name.
    window
        .cookies()
        .map_err(|_| "The provider sign-in could not be read.".to_owned())?
        .into_iter()
        .find(|cookie| cookie.name() == cookie_name)
        .map(|cookie| Zeroizing::new(cookie.value().to_owned()))
        .ok_or_else(|| "Sign in to the provider first, then try again.".to_owned())
}

/// Reads the credential out of the JSON-encoded `localStorage` value.
///
/// Evaluated scripts come back through `NSJSONSerialization`, so the payload
/// arrives as JSON text and a stored token reads back quoted, as `"token"`.
/// DeepSeek does not store a bare token: `userToken` holds an app-storage
/// envelope, `{"value":"<token>","__version":"0"}`, and therefore arrives as a
/// JSON string *containing* that envelope document. Only the outer layer is
/// plain JSON decoration, so the envelope has to be peeled as well - handing it
/// to the API verbatim is rejected as an invalid token, which surfaced as an
/// "expired session" immediately after a successful sign-in.
fn parse_local_storage_session(serialized: &str) -> Option<Zeroizing<String>> {
    let value = unwrap_session_payload(serialized)?;
    let value = value.trim();
    (!value.is_empty()).then(|| Zeroizing::new(value.to_owned()))
}

fn unwrap_session_payload(serialized: &str) -> Option<String> {
    let payload = match serde_json::from_str::<serde_json::Value>(serialized).ok()? {
        serde_json::Value::String(text) => text,
        envelope => return envelope_value(&envelope),
    };
    // Only an envelope document is peeled further. A token that merely looks
    // like JSON, such as `1234`, is returned verbatim rather than decoded away.
    match serde_json::from_str::<serde_json::Value>(&payload).ok() {
        Some(envelope @ serde_json::Value::Object(_)) => envelope_value(&envelope),
        _ => Some(payload),
    }
}

/// Reads the credential out of the app-storage envelope providers such as
/// DeepSeek wrap it in, e.g. `{"value":"<token>","__version":"0"}`.
fn envelope_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => object.get("value")?.as_str().map(str::to_owned),
        _ => None,
    }
}

async fn read_local_storage_session(
    window: &tauri::WebviewWindow,
    storage_key: &str,
) -> Result<Zeroizing<String>, String> {
    let key = serde_json::to_string(storage_key)
        .map_err(|_| "The provider sign-in could not be read.".to_owned())?;
    let script = format!(
        "(() => {{ try {{ return window.localStorage.getItem({key}); }} catch (_) {{ return null; }} }})()"
    );
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    window
        .eval_with_callback(script, move |value| {
            let _ = sender.send(value);
        })
        .map_err(|_| "The provider sign-in could not be read.".to_owned())?;
    let value = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .map_err(|_| "The provider sign-in could not be read.".to_owned())?
        .ok_or_else(|| "The provider sign-in could not be read.".to_owned())?;
    parse_local_storage_session(&value)
        .ok_or_else(|| "Sign in to the provider first, then try again.".to_owned())
}

async fn read_provider_session(
    window: &tauri::WebviewWindow,
    auth: &WebviewAuth,
) -> Result<Zeroizing<String>, String> {
    match &auth.credential {
        WebviewCredentialSource::Cookie { name } => read_cookie_session(window, name),
        WebviewCredentialSource::LocalStorage { key } => {
            read_local_storage_session(window, key).await
        }
    }
}

fn remove_local_storage_session(
    window: &tauri::WebviewWindow,
    storage_key: &str,
) -> Result<(), String> {
    let key = serde_json::to_string(storage_key)
        .map_err(|_| "The provider WebView session could not be removed.".to_owned())?;
    window
        .eval(format!(
            "try {{ window.localStorage.removeItem({key}); }} catch (_) {{}}"
        ))
        .map_err(|_| "The provider WebView session could not be removed.".to_owned())
}

/// 抓取失败时的诊断：记下窗口停在哪一页、两个 web storage 里各有哪些 key 名。
///
/// 只记 **key 名**，不记值——值就是凭据本身。URL 也只记 origin + path 和 query 的
/// **参数名**：登录回跳会把凭据放在 query 里，整条 URL 打进日志就等于泄露凭据。
async fn log_web_storage_diagnostic(window: &tauri::WebviewWindow) {
    let script = "(() => { try { return JSON.stringify({ \
        page: location.origin + location.pathname, \
        queryKeys: Array.from(new URLSearchParams(location.search).keys()), \
        sessionKeys: Object.keys(window.sessionStorage), \
        localKeys: Object.keys(window.localStorage) \
    }); } catch (_) { return null; } })()";
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    if window
        .eval_with_callback(script, move |value| {
            let _ = sender.send(value);
        })
        .is_err()
    {
        return;
    }
    if let Ok(Some(value)) = tokio::time::timeout(Duration::from_secs(3), receiver.recv()).await {
        crate::app_warn!("auth", "provider sign-in storage diagnostic: {value}");
    }
}

fn resolve_provider_link<'a>(
    registry: &'a ProviderRegistry,
    provider_id: &str,
    link_index: usize,
) -> Result<&'a ProviderLink, String> {
    registry
        .definition(provider_id)
        .and_then(|provider| provider.links.get(link_index))
        .ok_or_else(|| "That provider link is unavailable.".to_owned())
}

#[tauri::command]
pub fn open_provider_link(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
    link_index: usize,
) -> Result<(), String> {
    let link = resolve_provider_link(&registry, &provider_id, link_index)?;
    crate::app_debug!(
        "http",
        "opening {provider_id} provider link {}",
        crate::logging::redact_url(&link.url)
    );
    app.opener()
        .open_url(&link.url, None::<&str>)
        .map_err(|_| "That provider link could not be opened.".to_owned())
}

async fn api_key_state(
    registry: Arc<ProviderRegistry>,
    provider_id: String,
) -> Result<Option<ProviderApiKeyState>, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(status) = runtime.api_key_status() else {
            return Ok(None);
        };
        let status = status.map_err(|error| error.to_string())?;
        Ok(Some(ProviderApiKeyState {
            provider_id,
            status,
        }))
    })
    .await
    .map_err(|_| "The API key status could not be read.".to_owned())?
}

enum ApiKeyMutation<'a> {
    Save(&'a str),
    Delete,
}

struct AppliedApiKeyMutation {
    state: ProviderApiKeyState,
    status_uncertain: bool,
}

fn mutate_api_key(
    runtime: &dyn UsageProvider,
    provider_id: String,
    mutation: ApiKeyMutation<'_>,
) -> Result<AppliedApiKeyMutation, String> {
    let initial_status = runtime
        .api_key_status()
        .ok_or_else(|| "That provider does not accept an API key.".to_owned())?
        .ok();
    let fallback_status = match &mutation {
        ApiKeyMutation::Save(_) => {
            if matches!(
                initial_status,
                Some(
                    ApiKeyStatus::FromEnvironment
                        | ApiKeyStatus::FromConfig
                        | ApiKeyStatus::FromCliSignIn
                        | ApiKeyStatus::OverrideActive
                )
            ) {
                ApiKeyStatus::OverrideActive
            } else {
                ApiKeyStatus::Saved
            }
        }
        ApiKeyMutation::Delete => ApiKeyStatus::NotSet,
    };

    match mutation {
        ApiKeyMutation::Save(value) => runtime.save_api_key(value),
        ApiKeyMutation::Delete => runtime.delete_api_key(),
    }
    .map_err(|error| error.to_string())?;

    let (status, status_uncertain) = match runtime.api_key_status() {
        Some(Ok(status)) => (status, false),
        Some(Err(_)) | None => (fallback_status, true),
    };
    Ok(AppliedApiKeyMutation {
        state: ProviderApiKeyState {
            provider_id,
            status,
        },
        status_uncertain,
    })
}

pub(crate) fn reconcile_provider_credential_state(
    app: &AppHandle,
    service: &ProviderService,
    settings: &SettingsService,
    provider_id: &str,
    detected: bool,
    enable: bool,
) -> Result<(), String> {
    let updated = settings.reconcile_provider_credential_state(provider_id, detected, enable)?;
    tray_presentation::update(app, &service.state(), &updated, settings.registry());
    let _ = app.emit("settings-state", settings_view_state(app, settings));
    Ok(())
}

fn incomplete_mutation_warning(action: &str) -> String {
    format!(
        "The API key was {action}, but Quota01 could not finish updating provider status. Restart Quota01 or try again."
    )
}

#[tauri::command]
pub async fn get_provider_api_key_state(
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
) -> Result<Option<ProviderApiKeyState>, String> {
    api_key_state(registry.inner().clone(), provider_id).await
}

async fn provider_session_state(
    registry: Arc<ProviderRegistry>,
    provider_id: String,
) -> Result<Option<ProviderApiKeyState>, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(status) = runtime.session_status() else {
            return Ok(None);
        };
        let status = status.map_err(|error| error.to_string())?;
        Ok(Some(ProviderApiKeyState {
            provider_id,
            status,
        }))
    })
    .await
    .map_err(|_| "The connection status could not be read.".to_owned())?
}

#[tauri::command]
pub async fn get_provider_session_state(
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
) -> Result<Option<ProviderApiKeyState>, String> {
    provider_session_state(registry.inner().clone(), provider_id).await
}

#[tauri::command]
pub async fn open_provider_webview_login(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
) -> Result<(), String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let auth = runtime
        .webview_auth()
        .ok_or_else(|| "That provider does not use a WebView sign-in.".to_owned())?;

    if let Some(window) = app.get_webview_window(&auth.window_label) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let url = auth
        .login_url
        .parse::<tauri::Url>()
        .map_err(|_| "The provider sign-in URL is invalid.".to_owned())?;
    let provider_name = runtime.definition().display_name;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    // 查找与创建必须成对原子执行。同步命令跑在线程池上，两个并发调用会同时通过
    // 上面的存在性检查：Tauri 的存在性校验是 check-then-act（`prepare_window`），
    // 而窗口注册表只在 `Destroyed` 时清理，`CloseRequested` 上又装了
    // `prevent_close()`，中间那段窗口期足以让两次调用各建一个同 label 的窗口。
    // 投递到主线程后，这一整段被事件循环串行化，重复调用只会命中已存在的窗口。
    let event_app = app.clone();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            if let Some(window) = event_app.get_webview_window(&auth.window_label) {
                let _ = window.show();
                let _ = window.set_focus();
                return Ok(());
            }
            // 第三方登录（抖音等）用 `window.open` 弹出子窗口后才继续授权。wry 默认
            // 没有安装新窗口处理器，`createWebViewWithConfiguration` 会返回 `nil`，
            // 于是页面拿到的 `window.open()` 结果是 `null`：轮询回调永不触发，登录
            // 按钮就永远停在转圈状态，表现为「点登录没反应」。`Allow` 交给 wry 的
            // 默认实现弹出子窗口，页面才能拿到句柄把授权流程走完。
            let window = tauri::WebviewWindowBuilder::new(
                &event_app,
                &auth.window_label,
                WebviewUrl::External(url),
            )
            .title(format!("Sign in to {provider_name}"))
            .inner_size(1000.0, 720.0)
            .on_new_window(|_url, _features| tauri::webview::NewWindowResponse::Allow)
            .build()
            .map_err(|_| "The provider sign-in window could not be opened.".to_owned())?;
            register_provider_login_window(&window, &event_app, &provider_id, &auth);
            Ok(())
        })();
        let _ = sender.send(result);
    })
    .map_err(|_| "The provider sign-in window could not be opened.".to_owned())?;
    receiver
        .recv()
        .await
        .ok_or_else(|| "The provider sign-in window could not be opened.".to_owned())?
}

/// 关窗请求该被拦下（先抓取、抓完再关）还该直接放行。
///
/// 两种凭据来源都必须在窗口**还活着**的时候抓取：抓取函数第一步就是
/// `app.get_webview_window(LOGIN_WINDOW)`，拿不到窗口就直接报
/// `Open the provider sign-in window first.`。而 `Destroyed` 派发时 Tauri 已经在
/// `on_event_loop_event` 里调过 `manager.on_window_close`（见 tauri 的
/// `app.rs` / `manager/mod.rs`），窗口已从注册表摘掉，`Cookie` 类 provider 靠
/// `window.cookies()` 读凭据也再无窗口可读。所以两类 provider 都得在这里拦下。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseRequestDecision {
    /// 窗口已经由 Quota01 自己标记要关（抓取成功后的收尾），放行。
    Allow,
    /// 拦下关闭，交给前端抓取；抓完由抓取路径负责关窗。
    CaptureFirst,
}

fn close_request_decision(already_marked: bool) -> CloseRequestDecision {
    if already_marked {
        CloseRequestDecision::Allow
    } else {
        CloseRequestDecision::CaptureFirst
    }
}

/// 关闭被拦下后，最多等多久仍无人接手就放行。
///
/// 抓取是一次本地 cookie/storage 读取加一次 vault 写入，正常在毫秒级；给 5 秒足够
/// 覆盖慢盘与并发刷新，又不至于让用户以为窗口卡死。
const CLOSE_HOLD_GRACE: Duration = Duration::from_secs(5);

/// 给登录窗口装上事件钩子：用户关窗时先拦下，通知前端抓取凭据，抓完再关。
///
/// `Cookie` 类 provider（TraeWork CN）曾经不走这条路径，只依赖 `Destroyed`——那
/// 时窗口已经销毁，抓取必然失败且无声（`get_webview_window` 返回 `None` 是
/// `log_web_storage_diagnostic` 之前的提前返回，连日志都不留），表现为「网页里
/// 明明已登录、用量都能看到，卡片却一直要求登录」。两类凭据都走同一条路径。
fn register_provider_login_window(
    window: &tauri::WebviewWindow,
    app: &AppHandle,
    provider_id: &str,
    auth: &WebviewAuth,
) {
    let event_app = app.clone();
    let event_provider_id = provider_id.to_owned();
    let event_window_label = auth.window_label.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            crate::app_warn!("auth", "TEMP-DIAG close requested for {event_window_label}");
            let Some(close_guard) = event_app.try_state::<ProviderSessionCloseGuard>() else {
                crate::app_warn!("auth", "TEMP-DIAG no close guard state");
                return;
            };
            if close_request_decision(close_guard.is_marked(&event_window_label))
                == CloseRequestDecision::Allow
            {
                crate::app_warn!("auth", "TEMP-DIAG close allowed (marked)");
                close_guard.release_close(&event_window_label);
                return;
            }
            let now = Instant::now();
            // 等待期间用户又点了一次关闭：视为放弃抓取，直接放行，避免窗口关不掉。
            if close_guard.close_hold_expired(&event_window_label, now, CLOSE_HOLD_GRACE) {
                close_guard.release_close(&event_window_label);
                return;
            }
            let first_request = close_guard.hold_close(&event_window_label, now);
            api.prevent_close();
            crate::app_warn!("auth", "TEMP-DIAG close prevented, first={first_request}");
            if !first_request {
                return;
            }
            // 窗口要留到抓取读完 cookie 为止，但用户点的是「关闭」，不该看到一个
            // 关不掉的窗口。先隐藏：观感上就是关掉了，而 `window.cookies()` 只读
            // `WKWebsiteDataStore`，与窗口是否可见无关，抓取照常拿得到凭据。抓取
            // 结束的收尾会真正 `close()` 掉它。
            if let Some(window) = event_app.get_webview_window(&event_window_label) {
                let _ = window.hide();
            }
            crate::app_warn!("auth", "TEMP-DIAG emitting window-closed event");
            let _ = event_app.emit(
                PROVIDER_SESSION_WINDOW_CLOSED_EVENT,
                ProviderSessionWindowClosedEvent {
                    provider_id: event_provider_id.clone(),
                },
            );
        }
        WindowEvent::Destroyed => {
            let consumed = event_app
                .try_state::<ProviderSessionCloseGuard>()
                .is_some_and(|guard| {
                    let consumed = guard.consume(&event_window_label);
                    guard.release_close(&event_window_label);
                    consumed
                });
            if consumed {
                return;
            }
            // 走到这里说明窗口没经过 `CloseRequested`（Quota01 自己关的窗口会先
            // `mark`，于是 `consume` 命中并 return）。此时窗口已经销毁，抓取没有
            // 窗口可读，只能记一条诊断——否则这条路径完全无声。
            crate::app_warn!(
                "auth",
                "provider sign-in window {} was destroyed without a capture; \
                 credentials were not saved",
                event_window_label
            );
        }
        _ => {}
    });
}

#[tauri::command]
pub async fn capture_provider_session(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
) -> Result<ProviderApiKeyState, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let auth = runtime
        .webview_auth()
        .ok_or_else(|| "That provider does not use a WebView sign-in.".to_owned())?;

    let Some(guard) = app.try_state::<ProviderSessionCloseGuard>() else {
        return Err("The provider sign-in could not be read.".to_owned());
    };
    // 抓取权在这里取得并持有到函数结束。重复的并发抓取直接返回，不再去读同一个
    // 登录窗口——否则两边会各自读一次 storage/cookie，后到的失败还会把先到的成功
    // 覆盖成错误提示。
    let Some(_capture_guard) = guard.begin_capture(&provider_id) else {
        return Err("The provider sign-in is already being read.".to_owned());
    };

    capture_provider_session_inner(
        app.clone(),
        runtime,
        auth.clone(),
        service.inner().clone(),
        settings.inner().clone(),
        notifications.inner().clone(),
        provider_id,
    )
    .await
}

async fn capture_provider_session_inner(
    app: AppHandle,
    runtime: Arc<dyn UsageProvider>,
    auth: WebviewAuth,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    notifications: Arc<NotificationEvaluator>,
    provider_id: String,
) -> Result<ProviderApiKeyState, String> {
    let login_window = app.get_webview_window(&auth.window_label);
    // 登录窗口必须存在才能抓取。历史实现让 Cookie 类 provider 回退到 Main 窗口，
    // 但 Main 窗口加载的是 Quota01 自己的 `tauri://localhost` 页面：退出登录会先
    // `close()` 掉登录窗口，随后的抓取就落到 Main 窗口上，日志里记下的
    // `page: "tauri://localhost"` 正是这条错误路径。读错窗口不仅必然抓不到会话，
    // 还可能把无关 cookie 当成凭据存进 vault，所以这里只认登录窗口本身。
    let Some(session_window) = login_window.clone() else {
        // 这条路径必须留痕：`CaptureFirst` 的拦截保证窗口在抓取期间还活着，走到
        // 这里只可能是窗口被绕过 `CloseRequested` 关掉了（例如进程内直接
        // `close()`）。历史上它完全无声——正是它让「已登录却没存进 vault」查不出
        // 原因，所以诊断在这里同样要记。
        crate::app_warn!(
            "auth",
            "capture for {provider_id} found no sign-in window; credentials were not saved"
        );
        if let Some(close_guard) = app.try_state::<ProviderSessionCloseGuard>() {
            close_guard.release_close(&auth.window_label);
        }
        return Err("Open the provider sign-in window first.".to_owned());
    };
    let session = match read_provider_session(&session_window, &auth).await {
        Ok(session) => session,
        Err(error) => {
            // 抓取失败时先把关闭请求放行：否则关闭会被一直拦下、抓取又完成不了，
            // 用户就被卡在一个关不掉的窗口里。诊断随后记下，最后才报错。
            if let Some(close_guard) = app.try_state::<ProviderSessionCloseGuard>() {
                close_guard.mark(&auth.window_label);
                close_guard.release_close(&auth.window_label);
            }
            // 关窗时窗口已被隐藏。抓不到凭据意味着登录没完成，用户需要回到窗口里
            // 继续操作，所以把它重新显示出来；否则窗口看起来已经消失，用户会以
            // 为登录流程结束了，而卡片其实还在报「请先登录」。
            let _ = session_window.show();
            let _ = session_window.set_focus();
            log_web_storage_diagnostic(&session_window).await;
            return Err(error);
        }
    };

    let credential_guard = settings.lock_credential_mutation().await;
    settings.record_provider_credential_mutation();
    let runtime_for_save = runtime.clone();
    tauri::async_runtime::spawn_blocking(move || runtime_for_save.save_session(session.as_str()))
        .await
        .map_err(|_| "The provider sign-in could not be saved.".to_owned())?
        .map_err(|error| error.to_string())?;

    let command_guard = settings.lock_command_mutation().await;
    let settings_reconciled =
        reconcile_provider_credential_state(&app, &service, &settings, &provider_id, true, true)
            .is_ok();
    drop(command_guard);
    drop(credential_guard);

    if let Some(window) = login_window {
        let close_guard = app.try_state::<ProviderSessionCloseGuard>();
        if let Some(close_guard) = close_guard.as_ref() {
            close_guard.mark(&auth.window_label);
            close_guard.release_close(&auth.window_label);
        }
        if window.close().is_err() {
            if let Some(close_guard) = close_guard.as_ref() {
                close_guard.unmark(&auth.window_label);
            }
        }
    }
    service.refresh(&provider_id, true).await;
    let usage = service.state();
    let _ = app.emit("usage-state", &usage);
    finish_refresh(&app, &usage, &settings, &notifications);

    let status = runtime
        .session_status()
        .ok_or_else(|| "That provider does not use a WebView sign-in.".to_owned())?
        .map_err(|error| error.to_string())?;
    crate::app_info!("auth", "WebView session saved for {provider_id}");
    if !settings_reconciled {
        crate::app_warn!(
            "auth",
            "provider state after session save could not be reconciled for {provider_id}"
        );
    }
    Ok(ProviderApiKeyState {
        provider_id,
        status,
    })
}

/// 断开连接不再以 WebView 登录为前提：设备码登录把会话存进 Quota01 自己的 vault，
/// 这类 provider 没有 WebView 可清理（返回 `None`），但照样有连接要断开。
/// 两者都没有的 provider 才真的没有连接，此时给前端一句统一的说法。
fn disconnect_webview_auth(runtime: &dyn UsageProvider) -> Result<Option<WebviewAuth>, String> {
    let auth = runtime.webview_auth();
    if auth.is_none() && runtime.session_status().is_none() {
        return Err("That provider does not have a saved connection.".to_owned());
    }
    Ok(auth)
}

#[tauri::command]
pub async fn delete_provider_session(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
) -> Result<ProviderApiKeyState, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    // WebView 清理只对真用过 WebView 的 provider 有意义；会话存在 Quota01 自己
    // vault 里的 provider（设备码登录）从这里直接走后面与凭据来源无关的删除路径。
    let auth = disconnect_webview_auth(runtime.as_ref())?;
    if let Some(auth) = auth.as_ref() {
        let login_window = app.get_webview_window(&auth.window_label);
        match &auth.credential {
            WebviewCredentialSource::Cookie { name } => {
                // Read from the same dedicated cookie store used during capture; the
                // main window is the fallback when the sign-in window has closed.
                if let Some(cookie_window) = login_window
                    .clone()
                    .or_else(|| app.get_webview_window(crate::window::MAIN_WINDOW))
                {
                    if let Some(cookie) = cookie_window
                        .cookies()
                        .map_err(|_| "The provider WebView could not be read.".to_owned())?
                        .into_iter()
                        .find(|cookie| cookie.name() == name)
                    {
                        cookie_window.delete_cookie(cookie).map_err(|_| {
                            "The provider WebView session could not be removed.".to_owned()
                        })?;
                    }
                }
            }
            WebviewCredentialSource::LocalStorage { key } => {
                if let Some(window) = login_window.as_ref() {
                    remove_local_storage_session(window, key)?;
                }
            }
        }
        if let Some(window) = login_window {
            let close_guard = app.try_state::<ProviderSessionCloseGuard>();
            if let Some(close_guard) = close_guard.as_ref() {
                close_guard.mark(&auth.window_label);
                close_guard.release_close(&auth.window_label);
            }
            if window.close().is_err() {
                if let Some(close_guard) = close_guard.as_ref() {
                    close_guard.unmark(&auth.window_label);
                }
            }
        }
    }

    let credential_guard = settings.lock_credential_mutation().await;
    settings.record_provider_credential_mutation();
    let runtime_for_delete = runtime.clone();
    tauri::async_runtime::spawn_blocking(move || runtime_for_delete.delete_session())
        .await
        .map_err(|_| "The saved connection could not be removed.".to_owned())?
        .map_err(|error| error.to_string())?;

    let status = runtime
        .session_status()
        .ok_or_else(|| "That provider does not have a saved connection.".to_owned())?
        .map_err(|error| error.to_string())?;
    let detected = status != ApiKeyStatus::NotSet;
    let command_guard = settings.lock_command_mutation().await;
    let settings_reconciled = reconcile_provider_credential_state(
        &app,
        &service,
        &settings,
        &provider_id,
        detected,
        detected,
    )
    .is_ok();
    let should_refresh = settings
        .get()
        .providers
        .iter()
        .any(|provider| provider.id == provider_id && provider.enabled);
    drop(command_guard);
    drop(credential_guard);

    if should_refresh {
        service.refresh(&provider_id, true).await;
    }
    let usage = service.state();
    let _ = app.emit("usage-state", &usage);
    finish_refresh(&app, &usage, &settings, &notifications);
    crate::app_info!("auth", "WebView session removed for {provider_id}");
    if !settings_reconciled {
        crate::app_warn!(
            "auth",
            "provider state after session removal could not be reconciled for {provider_id}"
        );
    }
    Ok(ProviderApiKeyState {
        provider_id,
        status,
    })
}

#[tauri::command]
pub async fn save_provider_api_key(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
    api_key: String,
) -> Result<ApiKeyMutationOutcome, String> {
    let api_key = Zeroizing::new(api_key);
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let credential_guard = settings.lock_credential_mutation().await;
    settings.record_provider_credential_mutation();
    let provider_for_save = provider_id.clone();
    let applied = tauri::async_runtime::spawn_blocking(move || {
        mutate_api_key(
            runtime.as_ref(),
            provider_for_save,
            ApiKeyMutation::Save(api_key.as_str()),
        )
    })
    .await
    .map_err(|_| "The API key could not be saved.".to_owned())??;

    let command_guard = settings.lock_command_mutation().await;
    let settings_reconciled = match reconcile_provider_credential_state(
        &app,
        &service,
        &settings,
        &provider_id,
        true,
        true,
    ) {
        Ok(()) => true,
        Err(error) => {
            crate::app_warn!(
                "auth",
                "provider state after API key save could not be reconciled for {provider_id}: {error}"
            );
            false
        }
    };
    if applied.status_uncertain {
        crate::app_warn!(
            "auth",
            "API key status could not be confirmed after saving for {provider_id}"
        );
    }
    drop(command_guard);
    drop(credential_guard);
    service.refresh(&provider_id, true).await;
    let usage = service.state();
    let _ = app.emit("usage-state", &usage);
    finish_refresh(&app, &usage, &settings, &notifications);
    crate::app_info!("auth", "API key saved for {provider_id}");
    Ok(ApiKeyMutationOutcome {
        state: applied.state,
        warning: (applied.status_uncertain || !settings_reconciled)
            .then(|| incomplete_mutation_warning("saved securely")),
    })
}

#[tauri::command]
pub async fn delete_provider_api_key(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
) -> Result<ApiKeyMutationOutcome, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let credential_guard = settings.lock_credential_mutation().await;
    settings.record_provider_credential_mutation();
    let provider_for_delete = provider_id.clone();
    let applied = tauri::async_runtime::spawn_blocking(move || {
        mutate_api_key(
            runtime.as_ref(),
            provider_for_delete,
            ApiKeyMutation::Delete,
        )
    })
    .await
    .map_err(|_| "The API key could not be removed.".to_owned())??;

    let command_guard = settings.lock_command_mutation().await;
    let detected = applied.state.status != ApiKeyStatus::NotSet;
    let settings_reconciled = match reconcile_provider_credential_state(
        &app,
        &service,
        &settings,
        &provider_id,
        detected,
        false,
    ) {
        Ok(()) => true,
        Err(error) => {
            crate::app_warn!(
                "auth",
                "provider state after API key removal could not be reconciled for {provider_id}: {error}"
            );
            false
        }
    };
    if applied.status_uncertain {
        crate::app_warn!(
            "auth",
            "API key status could not be confirmed after removal for {provider_id}"
        );
    }
    let should_refresh = settings
        .get()
        .providers
        .iter()
        .any(|provider| provider.id == provider_id && provider.enabled);
    drop(command_guard);
    drop(credential_guard);
    if should_refresh {
        service.refresh(&provider_id, true).await;
        let usage = service.state();
        let _ = app.emit("usage-state", &usage);
        finish_refresh(&app, &usage, &settings, &notifications);
    }
    crate::app_info!("auth", "saved API key removed for {provider_id}");
    Ok(ApiKeyMutationOutcome {
        state: applied.state,
        warning: (applied.status_uncertain || !settings_reconciled)
            .then(|| incomplete_mutation_warning("removed")),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
        time::Instant,
    };

    use crate::{
        models::{
            ApiKeyStatus, MetricDefinition, MetricSection, MetricSource, ProviderDefinition,
            ProviderErrorKind, ProviderLink, ProviderSnapshot,
        },
        providers::{
            test_definition, DeviceCodeStubProvider, ProviderError, ProviderRegistry, StubProvider,
            UsageProvider, WebviewStubProvider,
        },
    };

    use super::{
        close_request_decision, disconnect_webview_auth, mutate_api_key,
        parse_local_storage_session, resolve_provider_link, ApiKeyMutation, CloseRequestDecision,
        ProviderSessionCloseGuard, CLOSE_HOLD_GRACE,
    };

    #[test]
    fn a_close_request_is_held_open_so_the_capture_can_still_read_the_window() {
        // Cookie 类 provider（TraeWork CN）曾只在 `Destroyed` 上抓取，而那时 Tauri
        // 已经在 `on_event_loop_event` 里调过 `manager.on_window_close`：窗口已从
        // 注册表摘掉，`get_webview_window` 返回 `None`，抓取以
        // `Open the provider sign-in window first.` 提前返回，连诊断日志都不留。
        // 结果是网页里已登录、用量可见，卡片却一直要求登录。窗口必须在
        // `CloseRequested` 上被拦下，抓取才有窗口可读。
        assert_eq!(
            close_request_decision(false),
            CloseRequestDecision::CaptureFirst,
            "an unmarked close request must be prevented so capture can read the window"
        );
    }

    #[test]
    fn a_close_request_quota01_initiated_is_allowed_through() {
        // 抓取成功后由抓取路径自己 `mark` 再关窗；若这里再拦一次，窗口就永远关不掉。
        assert_eq!(close_request_decision(true), CloseRequestDecision::Allow);
    }

    #[test]
    fn only_the_first_held_close_asks_the_frontend_to_capture() {
        // 通知前端去抓取只在首次拦下时发一次。用户在等待期间反复点关闭，不该重复
        // 触发抓取——那正是「后到的失败覆盖先到的成功」的来源。
        let guard = ProviderSessionCloseGuard::default();
        let start = Instant::now();

        assert!(guard.hold_close("trae-cn-login", start));
        assert!(!guard.hold_close("trae-cn-login", start));
    }

    #[test]
    fn a_close_request_left_unanswered_is_released_after_the_grace_period() {
        // 承载监听的组件只在卡片是错误态时才渲染。若没人接手抓取，拦截会永远解不
        // 开，窗口就变成一个关不掉的窗口。超过宽限期必须放行。
        let guard = ProviderSessionCloseGuard::default();
        let start = Instant::now();
        guard.hold_close("trae-cn-login", start);

        assert!(!guard.close_hold_expired("trae-cn-login", start, CLOSE_HOLD_GRACE));
        assert!(guard.close_hold_expired(
            "trae-cn-login",
            start + CLOSE_HOLD_GRACE,
            CLOSE_HOLD_GRACE
        ));
    }

    #[test]
    fn releasing_a_held_close_clears_the_grace_period() {
        // 放行/完成之后必须清掉记录，否则下一次打开登录窗口会带着上一次的时间戳，
        // 一进来就被判定为「早就过期」。
        let guard = ProviderSessionCloseGuard::default();
        let start = Instant::now();
        guard.hold_close("trae-cn-login", start);
        guard.release_close("trae-cn-login");

        assert!(!guard.close_hold_expired(
            "trae-cn-login",
            start + CLOSE_HOLD_GRACE * 2,
            CLOSE_HOLD_GRACE
        ));
    }

    #[test]
    fn a_second_concurrent_capture_of_the_same_provider_is_refused() {
        // 重复建窗曾让同一个 provider 的抓取并发跑两次，日志里两条诊断落在同一
        // 毫秒。第一次抓取持有句柄期间，第二次必须被拒绝。
        let guard = ProviderSessionCloseGuard::default();
        let first = guard.begin_capture("trae-cn");
        assert!(first.is_some());

        assert!(guard.begin_capture("trae-cn").is_none());
    }

    #[test]
    fn capture_rights_are_released_when_the_guard_is_dropped() {
        let guard = ProviderSessionCloseGuard::default();
        drop(
            guard
                .begin_capture("trae-cn")
                .expect("first capture is admitted"),
        );

        assert!(guard.begin_capture("trae-cn").is_some());
    }

    #[test]
    fn different_providers_capture_independently() {
        let guard = ProviderSessionCloseGuard::default();
        let trae = guard.begin_capture("trae-cn");
        let deepseek = guard.begin_capture("deepseek");

        assert!(trae.is_some());
        assert!(deepseek.is_some());
    }

    #[test]
    fn parses_and_trims_json_encoded_local_storage_session() {
        let parsed = parse_local_storage_session(r#""  user-token  ""#).unwrap();

        assert_eq!(parsed.as_str(), "user-token");
    }

    #[test]
    fn unwraps_enveloped_local_storage_sessions() {
        // Shapes `NSJSONSerialization` produces for DeepSeek's `userToken`: the
        // envelope document arrives as the payload of a JSON string.
        let quoted =
            parse_local_storage_session(r#""{\"value\":\"user-token\",\"__version\":\"0\"}""#)
                .unwrap();
        let bare =
            parse_local_storage_session(r#"{"value":"user-token","__version":"0"}"#).unwrap();

        assert_eq!(quoted.as_str(), "user-token");
        assert_eq!(bare.as_str(), "user-token");
    }

    #[test]
    fn keeps_tokens_that_look_like_json_verbatim() {
        assert_eq!(
            parse_local_storage_session(r#""1234""#).unwrap().as_str(),
            "1234"
        );
        assert_eq!(
            parse_local_storage_session(r#""true""#).unwrap().as_str(),
            "true"
        );
    }

    #[test]
    fn rejects_empty_or_invalid_local_storage_sessions() {
        assert!(parse_local_storage_session(r#""""#).is_none());
        assert!(parse_local_storage_session(r#""   ""#).is_none());
        assert!(parse_local_storage_session("null").is_none());
        assert!(parse_local_storage_session("42").is_none());
        assert!(parse_local_storage_session(r#"{"token":"value"}"#).is_none());
        assert!(parse_local_storage_session("not json").is_none());
    }

    #[test]
    fn rejects_envelopes_without_a_usable_value() {
        assert!(parse_local_storage_session(r#""{\"value\":\"   \"}""#).is_none());
        assert!(parse_local_storage_session(r#""{\"value\":42}""#).is_none());
        assert!(parse_local_storage_session(r#""{\"value\":null}""#).is_none());
        assert!(parse_local_storage_session(r#""{\"__version\":\"0\"}""#).is_none());
    }

    struct MutatingProvider {
        statuses: Mutex<VecDeque<Result<ApiKeyStatus, ProviderError>>>,
        saved_value: Mutex<Option<String>>,
        deleted: AtomicBool,
    }

    impl MutatingProvider {
        fn new(statuses: Vec<Result<ApiKeyStatus, ProviderError>>) -> Self {
            Self {
                statuses: Mutex::new(statuses.into()),
                saved_value: Mutex::new(None),
                deleted: AtomicBool::new(false),
            }
        }
    }

    impl UsageProvider for MutatingProvider {
        fn definition(&self) -> ProviderDefinition {
            registry().definition("provider").unwrap().clone()
        }

        fn has_local_credentials(&self) -> bool {
            false
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            unreachable!()
        }

        fn api_key_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
            Some(
                self.statuses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Ok(ApiKeyStatus::NotSet)),
            )
        }

        fn save_api_key(&self, value: &str) -> Result<(), ProviderError> {
            *self.saved_value.lock().unwrap() = Some(value.to_owned());
            Ok(())
        }

        fn delete_api_key(&self) -> Result<(), ProviderError> {
            self.deleted.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn registry() -> ProviderRegistry {
        ProviderRegistry::from_definitions(vec![ProviderDefinition {
            id: "provider".into(),
            display_name: "Provider".into(),
            short_name: "P".into(),
            fallback_enabled: true,
            local_usage_source_note: None,
            links: vec![ProviderLink::new("Status", "https://status.example.com/")],
            metrics: vec![MetricDefinition::new(
                "provider.session",
                "Session",
                MetricSource::Quota {
                    source_id: "session".into(),
                    session_window: true,
                },
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                Some("S"),
                None,
            )],
        }])
        .unwrap()
    }

    #[test]
    fn resolves_only_links_declared_by_the_provider_registry() {
        let registry = registry();

        assert_eq!(
            resolve_provider_link(&registry, "provider", 0).unwrap().url,
            "https://status.example.com/"
        );
        assert!(resolve_provider_link(&registry, "provider", 1).is_err());
        assert!(resolve_provider_link(&registry, "unknown", 0).is_err());
    }

    #[test]
    fn applied_api_key_save_is_not_reported_as_failed_when_status_refresh_fails() {
        let provider = MutatingProvider::new(vec![
            Ok(ApiKeyStatus::FromEnvironment),
            Err(ProviderError::new(
                ProviderErrorKind::CredentialStorage,
                "status unavailable",
            )),
        ]);

        let applied =
            mutate_api_key(&provider, "provider".into(), ApiKeyMutation::Save("secret")).unwrap();

        assert_eq!(applied.state.status, ApiKeyStatus::OverrideActive);
        assert!(applied.status_uncertain);
        assert_eq!(
            provider.saved_value.lock().unwrap().as_deref(),
            Some("secret")
        );
    }

    #[test]
    fn applied_api_key_delete_is_not_reported_as_failed_when_status_refresh_fails() {
        let provider = MutatingProvider::new(vec![
            Ok(ApiKeyStatus::Saved),
            Err(ProviderError::new(
                ProviderErrorKind::CredentialStorage,
                "status unavailable",
            )),
        ]);

        let applied = mutate_api_key(&provider, "provider".into(), ApiKeyMutation::Delete).unwrap();

        assert_eq!(applied.state.status, ApiKeyStatus::NotSet);
        assert!(applied.status_uncertain);
        assert!(provider.deleted.load(Ordering::SeqCst));
    }

    #[test]
    fn a_stored_session_without_a_webview_can_still_disconnect() {
        // 设备码登录的 provider（WorkBuddy）没有 WebView：会话在 Quota01 自己的
        // vault 里，清理 WebView 没意义，但断开连接必须照常走到 delete_session。
        let runtime = DeviceCodeStubProvider(test_definition("device-code"));

        assert_eq!(disconnect_webview_auth(&runtime), Ok(None));
    }

    #[test]
    fn a_webview_provider_keeps_its_cleanup_target() {
        let runtime = WebviewStubProvider(test_definition("webview"));

        assert_eq!(
            disconnect_webview_auth(&runtime),
            Ok(runtime.webview_auth())
        );
    }

    #[test]
    fn a_provider_without_a_webview_or_a_session_has_nothing_to_disconnect() {
        let runtime = StubProvider(test_definition("plain"));

        assert_eq!(
            disconnect_webview_auth(&runtime),
            Err("That provider does not have a saved connection.".to_owned())
        );
    }
}
