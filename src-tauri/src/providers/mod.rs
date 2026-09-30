use std::sync::Arc;

pub mod antigravity;
pub mod api_key;
pub mod claude;
pub mod codex;
pub mod commandcode;
pub mod copilot;
pub mod credential_store;
pub(crate) mod credential_vault;
pub mod cursor;
mod daily_usage;
pub mod deepseek;
mod detection;
pub mod grok;
// Provider migrations in follow-up tasks consume the factory through their request contexts.
#[allow(dead_code)]
pub mod http;
pub mod infini;
pub(crate) mod keychain_access;
pub mod kimi;
mod log_usage;
pub mod minimax;
pub mod opencode;
pub mod openrouter;
mod pi_usage;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod provider_icons;
mod registry;
pub mod siliconflow;
#[cfg(test)]
pub mod test_http;
pub mod trae;
pub mod workbuddy;
pub mod zai;

pub use detection::{detect_local_credentials, CredentialProbeResults, CredentialProbeStatus};
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) use provider_icons::provider_icon_svg;
#[cfg(test)]
pub(crate) use registry::normalize_default_pins;
pub use registry::ProviderRegistry;
#[cfg(test)]
pub(crate) use registry::{
    test_definition, DeviceCodeStubProvider, StubProvider, WebviewStubProvider,
    DEVICE_CODE_FAILED_LOGIN_ID, DEVICE_CODE_LOGIN_ID,
};

use crate::models::{
    ApiKeyStatus, DeviceCodeChallenge, DeviceCodePoll, ProviderDefinition, ProviderErrorKind,
    ProviderSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebviewCredentialSource {
    Cookie { name: String },
    LocalStorage { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebviewAuth {
    pub login_url: String,
    pub credential: WebviewCredentialSource,
    pub window_label: String,
}

/// 设备码登录的描述：`platform` 是申请 state 时上报的产品标识。
/// 具体流程（申请 state、轮询换 token、取账号资料）留在 provider 内部，
/// 前端只拿句柄和展示信息，永远碰不到 `state` 或 token。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodeAuth {
    pub platform: String,
}

pub fn provider_family(provider_id: &str) -> &str {
    provider_id
        .split_once('@')
        .map(|(family, _)| family)
        .unwrap_or(provider_id)
}

/// Provider ids that were renamed when the mainland-China variants were split out. The
/// `-cn` marker distinguishes a domestic site from its international sibling, so state
/// persisted under the old id must be carried over instead of being dropped as unknown.
pub(crate) const RENAMED_PROVIDER_IDS: &[(&str, &str)] =
    &[("kimi", "kimi-cn"), ("workbuddy", "workbuddy-cn")];

/// Resolves a provider id written by an older build to its current id. `None` means the
/// id was never renamed and should be used as-is.
pub(crate) fn migrated_provider_id(provider_id: &str) -> Option<&'static str> {
    RENAMED_PROVIDER_IDS
        .iter()
        .find(|(from, _)| *from == provider_id)
        .map(|(_, to)| *to)
}

pub fn is_claude_account_provider_id(provider_id: &str) -> bool {
    provider_id.strip_prefix("claude@").is_some_and(|suffix| {
        suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

pub fn remember_default_account(
    storage: &crate::storage::Storage,
    family: &str,
    identity: &str,
) -> Result<(), crate::storage::StorageError> {
    let records = storage.load_provider_account_records(family)?;
    if records
        .iter()
        .any(|(known_identity, provider_id, _)| known_identity == identity && provider_id == family)
    {
        return Ok(());
    }
    if records
        .iter()
        .any(|(known_identity, provider_id, _)| known_identity == identity || provider_id == family)
    {
        return Ok(());
    }
    storage.save_provider_account_record(family, identity, family, "{}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheIdentity<'a> {
    Unscoped,
    Resolved(&'a str),
    Unresolved,
}

pub struct AccountRefresh {
    pub family: &'static str,
    pub provider_id: &'static str,
    pub identity: String,
}

pub struct ProviderRefresh {
    pub snapshot: ProviderSnapshot,
    pub cache_identity: Option<String>,
    pub account: Option<AccountRefresh>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct ProviderRequestContext {
    pub proxy_url: Option<reqwest::Url>,
    pub http_clients: Arc<http::ProviderHttpClientFactory>,
}

impl ProviderRequestContext {
    pub fn direct(http_clients: Arc<http::ProviderHttpClientFactory>) -> Self {
        Self {
            proxy_url: None,
            http_clients,
        }
    }
}

impl<'a> CacheIdentity<'a> {
    pub fn resolved_value(self) -> Option<&'a str> {
        match self {
            Self::Resolved(value) => Some(value),
            Self::Unscoped | Self::Unresolved => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ProviderError {
    kind: ProviderErrorKind,
    message: String,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        let message = message.into();
        // An orphaned credential vault is not a provider problem and not
        // something retrying can fix. Providers report it through their own
        // `CredentialStorage` variant, so reclassify it here, at the single
        // point every provider error passes through, and keep the actionable
        // "reset the vault" wording instead of the provider's "API key could
        // not be read" text.
        if kind == ProviderErrorKind::CredentialStorage
            && credential_vault::is_unrecoverable_vault_error(&message)
        {
            return Self {
                kind: ProviderErrorKind::CredentialsUnavailable,
                message,
            };
        }
        Self { kind, message }
    }

    pub fn from_display(kind: ProviderErrorKind, error: impl std::fmt::Display) -> Self {
        Self::new(kind, error.to_string())
    }

    pub fn kind(&self) -> ProviderErrorKind {
        self.kind
    }
}

pub trait UsageProvider: Send + Sync {
    fn definition(&self) -> ProviderDefinition;
    fn has_local_credentials(&self) -> bool;
    fn has_local_installation(&self) -> bool {
        false
    }
    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError>;

    fn refresh_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<ProviderSnapshot, ProviderError> {
        let _ = context;
        self.refresh()
    }

    #[allow(dead_code)] // Retained for direct provider refresh call sites.
    fn refresh_for_service(&self) -> Result<ProviderRefresh, ProviderError> {
        let snapshot = self.refresh()?;
        Ok(ProviderRefresh {
            snapshot,
            cache_identity: self.cache_identity().resolved_value().map(str::to_owned),
            account: None,
        })
    }

    fn refresh_for_service_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<ProviderRefresh, ProviderError> {
        let snapshot = self.refresh_with_context(context)?;
        Ok(ProviderRefresh {
            snapshot,
            cache_identity: self.cache_identity().resolved_value().map(str::to_owned),
            account: None,
        })
    }

    fn cache_identity(&self) -> CacheIdentity<'_> {
        CacheIdentity::Unscoped
    }

    fn webview_auth(&self) -> Option<WebviewAuth> {
        None
    }

    /// 设备码登录的能力声明。`None` 表示这个 provider 不走设备码，
    /// registry 据此生成 `device_code_sign_in_provider_ids`，前端也据此决定是否显示登录面板。
    fn device_code_auth(&self) -> Option<DeviceCodeAuth> {
        None
    }

    fn start_device_code_login(&self) -> Result<DeviceCodeChallenge, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Internal,
            "That provider does not use a device-code sign-in.",
        ))
    }

    fn start_device_code_login_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<DeviceCodeChallenge, ProviderError> {
        let _ = context;
        self.start_device_code_login()
    }

    /// 轮询只回传「是否完成」和错误文案：会话在 provider 内部落库，
    /// token 绝不经过这条线进入前端。
    fn poll_device_code_login(&self, _login_id: &str) -> DeviceCodePoll {
        DeviceCodePoll {
            done: true,
            error: Some("That provider does not use a device-code sign-in.".to_owned()),
        }
    }

    fn poll_device_code_login_with_context(
        &self,
        login_id: &str,
        context: &ProviderRequestContext,
    ) -> DeviceCodePoll {
        let _ = context;
        self.poll_device_code_login(login_id)
    }

    fn cancel_device_code_login(&self, _login_id: &str) -> bool {
        false
    }

    fn session_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
        None
    }

    fn save_session(&self, _value: &str) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Internal,
            "That provider does not use a WebView session.",
        ))
    }

    fn delete_session(&self) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Internal,
            "That provider does not use a WebView session.",
        ))
    }

    fn supports_account_names(&self) -> bool {
        false
    }

    fn supports_api_key_configuration(&self) -> bool {
        false
    }

    fn account_identity(&self) -> Option<&str> {
        None
    }

    fn api_key_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
        None
    }

    fn save_api_key(&self, _value: &str) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Internal,
            "That provider does not accept an API key.",
        ))
    }

    fn delete_api_key(&self) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Internal,
            "That provider does not accept an API key.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        antigravity, claude, codex, commandcode, copilot, cursor, deepseek, grok, infini, kimi,
        minimax, opencode, openrouter, remember_default_account, siliconflow, trae, zai,
        ProviderError,
    };
    use crate::models::ProviderErrorKind;
    use tempfile::tempdir;

    #[test]
    fn remembered_default_account_is_stable_across_identity_changes() {
        let directory = tempdir().unwrap();
        let storage = crate::storage::Storage::open(&directory.path().join("quota01.db")).unwrap();

        remember_default_account(&storage, "codex", "identity-a").unwrap();
        remember_default_account(&storage, "codex", "identity-b").unwrap();

        assert_eq!(
            storage.load_provider_account_records("codex").unwrap(),
            [("identity-a".into(), "codex".into(), "{}".into())]
        );
    }

    #[test]
    fn an_orphaned_vault_is_reported_as_unavailable_credentials_not_a_broken_key() {
        // Every provider funnels credential-store failures through
        // `CredentialStorage`; the orphaned-vault cause must win, because a
        // retry cannot fix it and the recovery step is a vault reset.
        let error = ProviderError::new(
            ProviderErrorKind::CredentialStorage,
            super::credential_vault::UNRECOVERABLE_VAULT_ERROR,
        );
        assert_eq!(error.kind(), ProviderErrorKind::CredentialsUnavailable);
        assert_eq!(
            error.to_string(),
            super::credential_vault::UNRECOVERABLE_VAULT_ERROR
        );

        // An ordinary credential-store fault keeps its own kind.
        let ordinary = ProviderError::new(
            ProviderErrorKind::CredentialStorage,
            "System credential store unavailable.",
        );
        assert_eq!(ordinary.kind(), ProviderErrorKind::CredentialStorage);
    }

    #[test]
    fn provider_errors_expose_only_the_safe_message() {
        let error = ProviderError::new(
            ProviderErrorKind::Network,
            "Could not connect to the provider.",
        );

        assert_eq!(error.kind(), ProviderErrorKind::Network);
        assert_eq!(error.to_string(), "Could not connect to the provider.");
        assert!(!error.to_string().contains("secret-token"));
    }

    #[test]
    fn provider_quick_links_match_the_declared_browser_destinations() {
        let links = |definition: crate::models::ProviderDefinition| {
            definition
                .links
                .into_iter()
                .map(|link| (link.label, link.url))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            links(claude::definition()),
            [
                ("Status".into(), "https://status.anthropic.com/".into()),
                (
                    "Dashboard".into(),
                    "https://claude.ai/settings/usage".into()
                ),
            ]
        );
        assert_eq!(
            links(codex::definition()),
            [
                ("Status".into(), "https://status.openai.com/".into()),
                (
                    "Dashboard".into(),
                    "https://chatgpt.com/codex/settings/usage".into()
                ),
            ]
        );
        assert_eq!(
            links(cursor::definition()),
            [
                ("Status".into(), "https://status.cursor.com/".into()),
                (
                    "Dashboard".into(),
                    "https://www.cursor.com/dashboard".into()
                ),
            ]
        );
        assert!(links(antigravity::definition()).is_empty());
        assert_eq!(
            links(copilot::definition()),
            [
                ("Status".into(), "https://www.githubstatus.com/".into()),
                (
                    "Dashboard".into(),
                    "https://github.com/settings/billing".into()
                ),
            ]
        );
        assert_eq!(
            links(grok::definition()),
            [("Usage".into(), "https://grok.com/?_s=usage".into())]
        );
        assert_eq!(
            links(opencode::definition()),
            [("Dashboard".into(), "https://opencode.ai/auth".into())]
        );
        assert_eq!(
            links(openrouter::definition()),
            [
                ("Activity".into(), "https://openrouter.ai/activity".into()),
                (
                    "Credits".into(),
                    "https://openrouter.ai/settings/credits".into()
                ),
            ]
        );
        assert_eq!(
            links(zai::definition(zai::Site::Global)),
            [
                (
                    "Dashboard".into(),
                    "https://z.ai/manage-apikey/coding-plan/personal/my-plan".into()
                ),
                (
                    "API Keys".into(),
                    "https://z.ai/manage-apikey/apikey-list".into()
                ),
            ]
        );
        assert_eq!(
            links(zai::definition(zai::Site::Cn)),
            [
                (
                    "Dashboard".into(),
                    "https://open.bigmodel.cn/user-center/usage".into()
                ),
                (
                    "API Keys".into(),
                    "https://open.bigmodel.cn/user-center/apikeys".into()
                ),
            ]
        );
        assert_eq!(
            links(kimi::definition()),
            [
                (
                    "Dashboard".into(),
                    "https://www.kimi.com/code/console".into()
                ),
                (
                    "API Keys".into(),
                    "https://www.kimi.com/code/console".into()
                ),
            ]
        );
        assert_eq!(
            links(minimax::definition(minimax::Site::Global)),
            [
                (
                    "Dashboard".into(),
                    "https://platform.minimax.io/console/plan".into()
                ),
                (
                    "API Keys".into(),
                    "https://platform.minimax.io/console/access".into()
                ),
            ]
        );
        assert_eq!(
            links(minimax::definition(minimax::Site::Cn)),
            [
                (
                    "Dashboard".into(),
                    "https://platform.minimaxi.com/subscribe/token-plan".into()
                ),
                ("API Keys".into(), "https://platform.minimaxi.com/".into()),
            ]
        );
        assert_eq!(
            links(deepseek::definition()),
            [(
                "Dashboard".into(),
                "https://platform.deepseek.com/usage".into()
            )]
        );
        assert_eq!(
            links(siliconflow::definition(siliconflow::Site::Global)),
            [
                (
                    "Dashboard".into(),
                    "https://cloud.siliconflow.com/account/balance".into()
                ),
                (
                    "API Keys".into(),
                    "https://cloud.siliconflow.com/account/ak".into()
                ),
            ]
        );
        assert_eq!(
            links(siliconflow::definition(siliconflow::Site::Cn)),
            [
                (
                    "Dashboard".into(),
                    "https://cloud.siliconflow.cn/account/balance".into()
                ),
                (
                    "API Keys".into(),
                    "https://cloud.siliconflow.cn/account/ak".into()
                ),
            ]
        );
        assert_eq!(
            links(infini::definition()),
            [(
                "Dashboard".into(),
                "https://cloud.infini-ai.com/platform/ai".into()
            )]
        );
        assert_eq!(
            links(commandcode::definition()),
            [(
                "Dashboard".into(),
                "https://commandcode.ai/dashboard".into()
            )]
        );
        assert_eq!(
            links(trae::definition()),
            [(
                "Dashboard".into(),
                "https://www.trae.cn/account-setting#usage".into()
            )]
        );
    }
}
