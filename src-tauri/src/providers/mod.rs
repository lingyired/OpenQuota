pub mod antigravity;
pub mod api_key;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod credential_store;
pub(crate) mod credential_vault;
pub mod cursor;
mod daily_usage;
pub mod deepseek;
mod detection;
pub mod grok;
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

use crate::models::{ApiKeyStatus, ProviderDefinition, ProviderErrorKind, ProviderSnapshot};

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
        Self {
            kind,
            message: message.into(),
        }
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
    /// Whether this provider can read system credential store entries that belong to another
    /// application.
    ///
    /// macOS prompts for authorization on those reads, so every such read must go through
    /// `credential_store::read_external_password` (and writes through `write_external_password`),
    /// which stays inert until the user enables the provider by hand. Detecting existence with
    /// `generic_password_exists` does not prompt and stays ungated.
    fn accesses_system_keychain(&self) -> bool {
        false
    }
    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError>;

    fn refresh_for_service(&self) -> Result<ProviderRefresh, ProviderError> {
        let snapshot = self.refresh()?;
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
        antigravity, claude, codex, copilot, cursor, deepseek, grok, infini, kimi, minimax,
        opencode, openrouter, remember_default_account, siliconflow, trae, zai, ProviderError,
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
            links(trae::definition()),
            [(
                "Dashboard".into(),
                "https://www.trae.cn/account-setting#usage".into()
            )]
        );
    }
}
