use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::{
    models::{DeviceCodeChallenge, DeviceCodePoll},
    providers::ProviderRegistry,
};

/// 申请一次设备码登录。
///
/// R3：授权地址由 Rust 侧打开。前端永远不能把 URL 递给 opener，所以这里也只接受
/// provider id；挑战本身照常返回，界面还要用它显示地址和有效期。
#[tauri::command]
pub async fn start_provider_login(
    app: AppHandle,
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
) -> Result<DeviceCodeChallenge, String> {
    let challenge = start_provider_login_inner(registry.inner().clone(), provider_id).await?;
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
    provider_id: String,
) -> Result<DeviceCodeChallenge, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    // 申请要发一次网络请求，放在阻塞线程上，别占住异步运行时。
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .start_device_code_login()
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The sign-in could not be started.".to_owned())?
}

/// 轮询一次设备码登录。
///
/// 载荷里没有会话：token 在 provider 内部落库，这条线只有「完成了吗」和错误文案。
#[tauri::command]
pub async fn poll_provider_login(
    registry: State<'_, Arc<ProviderRegistry>>,
    provider_id: String,
    login_id: String,
) -> Result<DeviceCodePoll, String> {
    poll_provider_login_inner(registry.inner().clone(), provider_id, login_id).await
}

async fn poll_provider_login_inner(
    registry: Arc<ProviderRegistry>,
    provider_id: String,
    login_id: String,
) -> Result<DeviceCodePoll, String> {
    let runtime = registry
        .runtime(&provider_id)
        .ok_or_else(|| "Unknown provider.".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || runtime.poll_device_code_login(&login_id))
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
    use std::sync::Arc;

    use crate::{
        models::DeviceCodePoll,
        providers::{
            test_definition, DeviceCodeStubProvider, ProviderRegistry, StubProvider, UsageProvider,
            DEVICE_CODE_FAILED_LOGIN_ID, DEVICE_CODE_LOGIN_ID,
        },
    };

    use super::{
        cancel_provider_login_inner, poll_provider_login_inner, start_provider_login_inner,
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

    #[test]
    fn an_unknown_provider_cannot_start_a_login() {
        let error = tauri::async_runtime::block_on(start_provider_login_inner(
            registry(),
            "missing".into(),
        ))
        .unwrap_err();

        assert_eq!(error, "Unknown provider.");
    }

    #[test]
    fn a_provider_without_device_code_sign_in_cannot_start_a_login() {
        let error =
            tauri::async_runtime::block_on(start_provider_login_inner(registry(), "plain".into()))
                .unwrap_err();

        assert!(error.contains("device-code"), "unexpected message: {error}");
    }

    #[test]
    fn a_started_login_returns_exactly_what_the_provider_issued() {
        let expected = DeviceCodeStubProvider(test_definition("device-code"))
            .start_device_code_login()
            .unwrap();

        let challenge = tauri::async_runtime::block_on(start_provider_login_inner(
            registry(),
            "device-code".into(),
        ))
        .unwrap();

        assert_eq!(challenge, expected);
        assert_eq!(challenge.login_id, DEVICE_CODE_LOGIN_ID);
    }

    #[test]
    fn an_unknown_provider_cannot_poll_a_login() {
        let error = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry(),
            "missing".into(),
            DEVICE_CODE_LOGIN_ID.into(),
        ))
        .unwrap_err();

        assert_eq!(error, "Unknown provider.");
    }

    #[test]
    fn polling_forwards_the_login_id_to_the_provider() {
        let completed = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry(),
            "device-code".into(),
            DEVICE_CODE_LOGIN_ID.into(),
        ))
        .unwrap();
        let pending = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry(),
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
        let poll = tauri::async_runtime::block_on(poll_provider_login_inner(
            registry(),
            "device-code".into(),
            DEVICE_CODE_FAILED_LOGIN_ID.into(),
        ))
        .unwrap();

        assert!(poll.done);
        assert!(poll.error.is_some());
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
