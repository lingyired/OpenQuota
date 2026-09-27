use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::{
    commands::provider::reconcile_provider_credential_state,
    models::{DeviceCodeChallenge, DeviceCodePoll},
    notifications::finish_refresh,
    pacing::NotificationEvaluator,
    providers::ProviderRegistry,
    service::ProviderService,
    settings::SettingsService,
};

/// 申请一次设备码登录。
///
/// R3：授权地址由 Rust 侧打开。前端永远不能把 URL 递给 opener，所以这里也只接受
/// provider id；挑战本身照常返回，界面还要用它显示地址和有效期。
#[tauri::command]
pub async fn start_provider_login(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    provider_id: String,
) -> Result<DeviceCodeChallenge, String> {
    let service = service.inner().clone();
    let challenge =
        start_provider_login_inner(registry.inner().clone(), service, provider_id).await?;
    // 打开失败不回滚这次登录：挑战已经申请好了，界面仍能显示地址让用户手动打开，
    // 把整条命令变成错误反而会让这次登录的 login_id 一起丢掉。
    //
    // R23：登录层已经把非 https 的地址判为无效响应，这里再挡一次，是为了让登录层
    // 校验被放宽时也不会把网络给的 scheme 交给系统默认处理器。拒绝只记录（不打印地址本身），
    // 挑战照常返回，界面仍然可以显示链接。
    if challenge.verification_uri.starts_with("https://") {
        if app
            .opener()
            .open_url(&challenge.verification_uri, None::<&str>)
            .is_err()
        {
            crate::app_warn!("auth", "the device-code sign-in page could not be opened");
        }
    } else {
        crate::app_warn!(
            "auth",
            "the device-code sign-in page was not opened because its address is not https"
        );
    }
    Ok(challenge)
}

async fn start_provider_login_inner(
    registry: Arc<ProviderRegistry>,
    service: Arc<ProviderService>,
    provider_id: String,
) -> Result<DeviceCodeChallenge, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let context = service.request_context_for(&provider_id);
    // 申请要发一次网络请求，放在阻塞线程上，别占住异步运行时。
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .start_device_code_login_with_context(&context)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The sign-in could not be started.".to_owned())?
}

/// 轮询一次设备码登录。
///
/// 载荷里没有会话：token 在 provider 内部落库，这条线只有「完成了吗」和错误文案。
/// 但凭据一旦真的落库，这里就必须跟上所有会话命令共有的收尾：否则面板已经显示
/// 「已连接」，仪表盘却还停在登录前的错误与旧数据上，要等下一次定时刷新才更新。
#[tauri::command]
pub async fn poll_provider_login(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    service: State<'_, Arc<ProviderService>>,
    settings: State<'_, Arc<SettingsService>>,
    notifications: State<'_, Arc<NotificationEvaluator>>,
    provider_id: String,
    login_id: String,
) -> Result<DeviceCodePoll, String> {
    let service = service.inner().clone();
    let login_service = Arc::clone(&service);
    let settings = settings.inner().clone();
    let notifications = notifications.inner().clone();
    let settle_provider_id = provider_id.clone();
    poll_provider_login_and_settle(
        registry.inner().clone(),
        login_service,
        provider_id,
        login_id,
        move || async move {
            finish_provider_login(
                &app,
                &service,
                &settings,
                &notifications,
                &settle_provider_id,
            )
            .await;
        },
    )
    .await
}

/// 只有在轮询报告「已完成且没有错误」时才执行凭据变更后的收尾。
///
/// 带错误的 `done`（过期、域被拒、写库失败）说明 vault 里没有新凭据，
/// 未完成的 `done: false` 更是如此；这两种结果触发刷新只会把旧状态再播一遍。
async fn poll_provider_login_and_settle<F, Fut>(
    registry: Arc<ProviderRegistry>,
    service: Arc<ProviderService>,
    provider_id: String,
    login_id: String,
    credentials_stored: F,
) -> Result<DeviceCodePoll, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let poll = poll_provider_login_inner(registry, service, provider_id, login_id).await?;
    if poll.done && poll.error.is_none() {
        credentials_stored().await;
    }
    Ok(poll)
}

/// 凭据变更后的收尾，顺序与 `capture_provider_session` / `delete_provider_session`
/// 相同：凭据变更守卫、重算 provider 状态、按设置决定是否刷新、广播用量状态。
async fn finish_provider_login(
    app: &AppHandle,
    service: &Arc<ProviderService>,
    settings: &SettingsService,
    notifications: &NotificationEvaluator,
    provider_id: &str,
) {
    let credential_guard = settings.lock_credential_mutation().await;
    settings.record_provider_credential_mutation();
    let command_guard = settings.lock_command_mutation().await;
    let settings_reconciled =
        reconcile_provider_credential_state(app, service, settings, provider_id, true, true)
            .is_ok();
    let should_refresh = settings
        .get()
        .providers
        .iter()
        .any(|provider| provider.id == provider_id && provider.enabled);
    drop(command_guard);
    drop(credential_guard);

    if should_refresh {
        service.refresh(provider_id, true).await;
    }
    let usage = service.state();
    let _ = app.emit("usage-state", &usage);
    finish_refresh(app, &usage, settings, notifications);
    crate::app_info!("auth", "device-code session saved for {provider_id}");
    if !settings_reconciled {
        crate::app_warn!(
            "auth",
            "provider state after device-code session save could not be reconciled for {provider_id}"
        );
    }
}

async fn poll_provider_login_inner(
    registry: Arc<ProviderRegistry>,
    service: Arc<ProviderService>,
    provider_id: String,
    login_id: String,
) -> Result<DeviceCodePoll, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    let context = service.request_context_for(&provider_id);
    tauri::async_runtime::spawn_blocking(move || {
        runtime.poll_device_code_login_with_context(&login_id, &context)
    })
    .await
    .map_err(|_| "The sign-in status could not be read.".to_owned())
}

/// 放弃一次设备码登录；返回这次尝试此前是否仍在进行中。
#[tauri::command]
pub fn cancel_provider_login(
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
    login_id: String,
) -> Result<bool, String> {
    cancel_provider_login_inner(registry.inner(), &provider_id, &login_id)
}

fn cancel_provider_login_inner(
    registry: &ProviderRegistry,
    provider_id: &str,
    login_id: &str,
) -> Result<bool, String> {
    registry
        .runtime(provider_id)
        .map(|runtime| runtime.cancel_device_code_login(login_id))
        .ok_or_else(|| "Unknown provider.".to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::{
        models::DeviceCodePoll,
        providers::{
            test_definition, DeviceCodeStubProvider, ProviderRegistry, StubProvider, UsageProvider,
            DEVICE_CODE_FAILED_LOGIN_ID, DEVICE_CODE_LOGIN_ID,
        },
        service::ProviderService,
        storage::Storage,
    };

    use super::{
        cancel_provider_login_inner, poll_provider_login_and_settle, poll_provider_login_inner,
        start_provider_login_inner,
    };

    /// 真实 registry：一个没有设备码能力的 provider，加一个能走完整流程的桩。
    fn registry() -> Arc<ProviderRegistry> {
        Arc::new(
            ProviderRegistry::new(vec![
                Arc::new(StubProvider(test_definition("plain"))) as Arc<dyn UsageProvider>,
                Arc::new(DeviceCodeStubProvider(test_definition("device-code"))),
            ])
            .unwrap(),
        )
    }

    fn service(registry: Arc<ProviderRegistry>) -> (Arc<ProviderService>, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("provider-login.db")).unwrap());
        (Arc::new(ProviderService::new(registry, storage)), directory)
    }

    #[test]
    fn an_unknown_provider_cannot_start_a_login() {
        let registry = registry();
        let (service, _directory) = service(registry.clone());
        let error = tauri::async_runtime::block_on(start_provider_login_inner(
            registry,
            service,
            "missing".into(),
        ))
        .unwrap_err();

        assert_eq!(error, "Unknown provider.");
    }

    #[test]
    fn a_provider_without_device_code_sign_in_cannot_start_a_login() {
        let registry = registry();
        let (service, _directory) = service(registry.clone());
        let error = tauri::async_runtime::block_on(start_provider_login_inner(
            registry,
            service,
            "plain".into(),
        ))
        .unwrap_err();

        assert!(error.contains("device-code"), "unexpected message: {error}");
    }

    #[test]
    fn a_started_login_returns_exactly_what_the_provider_issued() {
        let expected = DeviceCodeStubProvider(test_definition("device-code"))
            .start_device_code_login()
            .unwrap();

        let registry = registry();
        let (service, _directory) = service(registry.clone());
        let challenge = tauri::async_runtime::block_on(start_provider_login_inner(
            registry,
            service,
            "device-code".into(),
        ))
        .unwrap();

        assert_eq!(challenge, expected);
        assert_eq!(challenge.login_id, DEVICE_CODE_LOGIN_ID);
    }

    #[test]
    fn an_unknown_provider_cannot_poll_a_login() {
        let registry = registry();
        let (service, _directory) = service(registry.clone());
        let error = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry,
            service,
            "missing".into(),
            DEVICE_CODE_LOGIN_ID.into(),
        ))
        .unwrap_err();

        assert_eq!(error, "Unknown provider.");
    }

    #[test]
    fn polling_forwards_the_login_id_to_the_provider() {
        let registry = registry();
        let (service, _directory) = service(registry.clone());
        let completed = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry.clone(),
            service.clone(),
            "device-code".into(),
            DEVICE_CODE_LOGIN_ID.into(),
        ))
        .unwrap();
        let pending = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry,
            service,
            "device-code".into(),
            "another-login".into(),
        ))
        .unwrap();

        assert_eq!(
            completed,
            DeviceCodePoll {
                done: true,
                error: None,
            }
        );
        assert_eq!(
            pending,
            DeviceCodePoll {
                done: false,
                error: None,
            }
        );
    }

    #[test]
    fn a_failed_attempt_travels_as_poll_data_instead_of_a_command_error() {
        let registry = registry();
        let (service, _directory) = service(registry.clone());
        let poll = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry,
            service,
            "device-code".into(),
            DEVICE_CODE_FAILED_LOGIN_ID.into(),
        ))
        .unwrap();

        assert!(poll.done);
        assert!(poll.error.is_some());
    }

    /// 轮询结果里只有 `done: true, error: None` 意味着 provider 把会话写进了
    /// Quota01 自己的 vault；收尾（凭据变更守卫、重算、刷新）必须只在这时跑一次。
    #[test]
    fn a_completed_poll_settles_the_stored_credentials_exactly_once() {
        let settled = Arc::new(AtomicUsize::new(0));
        let counter = settled.clone();
        let registry = registry();
        let (service, _directory) = service(registry.clone());

        let poll = tauri::async_runtime::block_on(poll_provider_login_and_settle(
            registry,
            service,
            "device-code".into(),
            DEVICE_CODE_LOGIN_ID.into(),
            move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        ))
        .unwrap();

        assert_eq!(
            poll,
            DeviceCodePoll {
                done: true,
                error: None,
            }
        );
        assert_eq!(settled.load(Ordering::SeqCst), 1);
    }

    /// 带错误的 `done`（过期、域被拒、写库失败）说明 vault 里没有新凭据，
    /// 此时刷新只会把旧状态再播一遍。
    #[test]
    fn a_failed_poll_does_not_settle_credentials() {
        let settled = Arc::new(AtomicUsize::new(0));
        let counter = settled.clone();
        let registry = registry();
        let (service, _directory) = service(registry.clone());

        let poll = tauri::async_runtime::block_on(poll_provider_login_and_settle(
            registry,
            service,
            "device-code".into(),
            DEVICE_CODE_FAILED_LOGIN_ID.into(),
            move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        ))
        .unwrap();

        assert!(poll.done);
        assert!(poll.error.is_some());
        assert_eq!(settled.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_pending_poll_does_not_settle_credentials() {
        let settled = Arc::new(AtomicUsize::new(0));
        let counter = settled.clone();
        let registry = registry();
        let (service, _directory) = service(registry.clone());

        let poll = tauri::async_runtime::block_on(poll_provider_login_and_settle(
            registry,
            service,
            "device-code".into(),
            "another-login".into(),
            move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        ))
        .unwrap();

        assert!(!poll.done);
        assert!(poll.error.is_none());
        assert_eq!(settled.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_unknown_provider_cannot_cancel_a_login() {
        let error =
            cancel_provider_login_inner(&registry(), "missing", DEVICE_CODE_LOGIN_ID).unwrap_err();

        assert_eq!(error, "Unknown provider.");
    }

    #[test]
    fn cancelling_forwards_the_login_id_to_the_provider() {
        let registry = registry();

        assert_eq!(
            cancel_provider_login_inner(&registry, "device-code", DEVICE_CODE_LOGIN_ID),
            Ok(true)
        );
        assert_eq!(
            cancel_provider_login_inner(&registry, "device-code", "another-login"),
            Ok(false)
        );
    }

    #[test]
    fn a_completed_attempt_cannot_be_cancelled_twice() {
        let registry = registry();

        assert_eq!(
            cancel_provider_login_inner(&registry, "device-code", DEVICE_CODE_FAILED_LOGIN_ID),
            Ok(false)
        );
    }
}
