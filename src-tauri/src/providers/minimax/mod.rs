mod auth;
mod client;
mod mapper;

use std::sync::Arc;

use chrono::Utc;
use reqwest::StatusCode;
use thiserror::Error;

use crate::models::{
    ApiKeyStatus, MetricDefinition, MetricSection, ProviderDefinition, ProviderErrorKind,
    ProviderLink, ProviderSnapshot, UsageHistory,
};

use self::{
    auth::MiniMaxAuthStore,
    client::{EndpointResponse, MiniMaxClient},
    mapper::map_usage,
};

use super::{ProviderError, UsageProvider};

/// MiniMax runs the same Token Plan backend for the international site
/// (`minimax.io`) and the mainland-China site (`minimaxi.com`). The endpoints and
/// response shapes match, so one implementation serves either site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    Global,
    Cn,
}

impl Site {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Global => "minimax",
            Self::Cn => "minimax-cn",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Global => "MiniMax",
            Self::Cn => "MiniMax CN",
        }
    }

    pub(crate) const fn short_name(self) -> &'static str {
        match self {
            Self::Global => "M",
            Self::Cn => "MC",
        }
    }

    pub(crate) const fn environment_names(self) -> &'static [&'static str] {
        match self {
            Self::Global => &["MINIMAX_API_KEY"],
            Self::Cn => &["MINIMAX_CN_API_KEY"],
        }
    }

    pub(crate) const fn config_paths(self) -> &'static [&'static str] {
        match self {
            Self::Global => &["~/.config/quota01/minimax.json"],
            Self::Cn => &["~/.config/quota01/minimax-cn.json"],
        }
    }

    pub(crate) const fn remains_url(self) -> &'static str {
        match self {
            Self::Global => "https://www.minimax.io/v1/token_plan/remains",
            Self::Cn => "https://www.minimaxi.com/v1/token_plan/remains",
        }
    }

    pub(crate) const fn missing_key_message(self) -> &'static str {
        match self {
            Self::Global => "Add a MiniMax API key in Customize or set MINIMAX_API_KEY.",
            Self::Cn => "Add a MiniMax CN API key in Customize or set MINIMAX_CN_API_KEY.",
        }
    }

    pub(crate) const fn invalid_key_message(self) -> &'static str {
        match self {
            Self::Global => "The MiniMax API key is invalid. Check it at minimax.io.",
            Self::Cn => "The MiniMax CN API key is invalid. Check it at platform.minimaxi.com.",
        }
    }

    pub(crate) const fn unreadable_key_message(self) -> &'static str {
        match self {
            Self::Global => "The MiniMax API key could not be read or updated.",
            Self::Cn => "The MiniMax CN API key could not be read or updated.",
        }
    }

    pub(crate) const fn no_token_plan_message(self) -> &'static str {
        match self {
            Self::Global => "No active MiniMax token plan. Subscribe at minimax.io to view usage.",
            Self::Cn => "No active MiniMax token plan. Subscribe at minimaxi.com to view usage.",
        }
    }

    fn links(self) -> Vec<ProviderLink> {
        match self {
            Self::Global => vec![
                ProviderLink::new("Dashboard", "https://platform.minimax.io/console/plan"),
                ProviderLink::new("API Keys", "https://platform.minimax.io/console/access"),
            ],
            Self::Cn => vec![
                ProviderLink::new(
                    "Dashboard",
                    "https://platform.minimaxi.com/subscribe/token-plan",
                ),
                ProviderLink::new("API Keys", "https://platform.minimaxi.com/"),
            ],
        }
    }
}

pub(crate) fn definition(site: Site) -> ProviderDefinition {
    ProviderDefinition {
        id: site.id().into(),
        display_name: site.display_name().into(),
        short_name: site.short_name().into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: site.links(),
        metrics: vec![
            MetricDefinition::quota(
                &format!("{}.session", site.id()),
                "Session",
                "session",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "S",
            ),
            MetricDefinition::quota(
                &format!("{}.weekly", site.id()),
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "W",
            ),
        ],
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum MiniMaxError {
    #[error("{}", .0.missing_key_message())]
    MissingKey(Site),
    #[error("{}", .0.invalid_key_message())]
    InvalidKey(Site),
    #[error("Could not reach MiniMax. Check your internet connection.")]
    ConnectionFailed,
    #[error("MiniMax usage data is temporarily unavailable.")]
    InvalidResponse,
    #[error("MiniMax request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("{}", .0.no_token_plan_message())]
    NoTokenPlan(Site),
    #[error("{}", .0.unreadable_key_message())]
    CredentialStorage(Site),
}

impl From<MiniMaxError> for ProviderError {
    fn from(error: MiniMaxError) -> Self {
        let kind = match error {
            MiniMaxError::MissingKey(_) | MiniMaxError::InvalidKey(_) => {
                ProviderErrorKind::Authentication
            }
            MiniMaxError::ConnectionFailed => ProviderErrorKind::Network,
            MiniMaxError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            MiniMaxError::RequestFailed(401 | 403) => ProviderErrorKind::Authentication,
            MiniMaxError::NoTokenPlan(_) => ProviderErrorKind::Permission,
            MiniMaxError::RequestFailed(_) | MiniMaxError::InvalidResponse => {
                ProviderErrorKind::InvalidResponse
            }
            MiniMaxError::CredentialStorage(_) => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::new(kind, error.to_string())
    }
}

pub struct MiniMaxProvider {
    site: Site,
    auth: MiniMaxAuthStore,
    client: Arc<MiniMaxClient>,
}

impl MiniMaxProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Self::with_site(Site::Global)
    }

    fn with_site(site: Site) -> Result<Self, ProviderError> {
        Ok(Self {
            site,
            auth: MiniMaxAuthStore::new(site),
            client: Arc::new(MiniMaxClient::new(site).map_err(ProviderError::from)?),
        })
    }

    #[cfg(test)]
    fn with_dependencies(auth: MiniMaxAuthStore, client: MiniMaxClient) -> Self {
        Self {
            site: Site::Global,
            auth,
            client: Arc::new(client),
        }
    }

    fn refresh_snapshot(&self, api_key: &str) -> Result<ProviderSnapshot, ProviderError> {
        let response = required_response(self.site, self.client.fetch(api_key))?;
        let mapped = map_usage(self.site, &response.body)?;
        Ok(ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: self.site.id().into(),
            plan: mapped.plan,
            quotas: mapped.quotas,
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        })
    }
}

impl UsageProvider for MiniMaxProvider {
    fn definition(&self) -> ProviderDefinition {
        definition(self.site)
    }

    fn has_local_credentials(&self) -> bool {
        self.auth.has_local_credentials()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        let api_key = self
            .auth
            .load()
            .map_err(ProviderError::from)?
            .ok_or_else(|| ProviderError::from(MiniMaxError::MissingKey(self.site)))?;
        self.refresh_snapshot(api_key.as_str())
    }

    fn api_key_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
        Some(self.auth.status().map_err(ProviderError::from))
    }

    fn supports_api_key_configuration(&self) -> bool {
        true
    }

    fn save_api_key(&self, value: &str) -> Result<(), ProviderError> {
        self.auth.save(value).map_err(ProviderError::from)
    }

    fn delete_api_key(&self) -> Result<(), ProviderError> {
        self.auth.delete().map_err(ProviderError::from)
    }
}

/// Mainland-China runtime for the `minimaxi.com` Token Plan. It shares every code
/// path with [`MiniMaxProvider`]; only the site (id, endpoint, credential
/// sources) differs.
pub struct MiniMaxCnProvider(MiniMaxProvider);

impl MiniMaxCnProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self(MiniMaxProvider::with_site(Site::Cn)?))
    }
}

impl UsageProvider for MiniMaxCnProvider {
    fn definition(&self) -> ProviderDefinition {
        definition(Site::Cn)
    }

    fn has_local_credentials(&self) -> bool {
        self.0.has_local_credentials()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        self.0.refresh()
    }

    fn api_key_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
        self.0.api_key_status()
    }

    fn supports_api_key_configuration(&self) -> bool {
        true
    }

    fn save_api_key(&self, value: &str) -> Result<(), ProviderError> {
        self.0.save_api_key(value)
    }

    fn delete_api_key(&self) -> Result<(), ProviderError> {
        self.0.delete_api_key()
    }
}

fn required_response(
    site: Site,
    response: Result<EndpointResponse, MiniMaxError>,
) -> Result<EndpointResponse, MiniMaxError> {
    let response = response?;
    if matches!(
        response.status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(MiniMaxError::InvalidKey(site));
    }
    if !response.status.is_success() {
        return Err(MiniMaxError::RequestFailed(response.status.as_u16()));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use crate::{
        models::ProviderErrorKind,
        providers::{api_key::*, test_http, UsageProvider},
    };

    use super::{auth::MiniMaxAuthStore, client::MiniMaxClient, definition, MiniMaxProvider, Site};

    #[derive(Default)]
    struct MemorySecrets(Mutex<HashMap<String, Vec<u8>>>);

    impl SecretBackend for MemorySecrets {
        fn read(&self, account: &str) -> Result<Option<SecretBytes>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .map(SecretBytes::new))
        }
        fn write(&self, account: &str, value: &[u8]) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .insert(account.to_owned(), value.to_vec());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), String> {
            self.0.lock().unwrap().remove(account);
            Ok(())
        }
    }

    struct Environment(HashMap<String, String>);
    impl EnvironmentReader for Environment {
        fn value(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    fn auth(key: Option<&str>) -> MiniMaxAuthStore {
        MiniMaxAuthStore::with_store(
            Site::Global,
            ApiKeyStore::with_backends(
                "minimax",
                "MINIMAX_API_KEY",
                Arc::new(MemorySecrets::default()),
                Arc::new(Environment(
                    key.map(|value| HashMap::from([("MINIMAX_API_KEY".into(), value.into())]))
                        .unwrap_or_default(),
                )),
            ),
        )
    }

    const REMAINS_BODY: &str = r#"{"model_remains":[{
        "start_time":1786060800000,"end_time":1786078800000,"remains_time":2185461,
        "current_interval_total_count":0,"current_interval_usage_count":0,"model_name":"general",
        "current_weekly_total_count":0,"current_weekly_usage_count":0,
        "weekly_start_time":1785715200000,"weekly_end_time":1786320000000,"weekly_remains_time":243385461,
        "current_interval_status":2,"current_interval_remaining_percent":0,
        "current_weekly_status":3,"current_weekly_remaining_percent":100}],
        "base_resp":{"status_code":0,"status_msg":"success"}}"#;

    #[test]
    fn refresh_maps_weekly_and_interval() {
        let url = test_http::serve_once(200, &[], REMAINS_BODY);
        let provider = MiniMaxProvider::with_dependencies(
            auth(Some("secret")),
            MiniMaxClient::for_test(&url, Duration::from_secs(1)),
        );

        let snapshot = provider.refresh().unwrap();
        assert_eq!(snapshot.provider_id, "minimax");
        assert_eq!(snapshot.plan.as_deref(), Some("Token Plan"));
        assert_eq!(
            snapshot
                .quotas
                .iter()
                .map(|quota| quota.id.as_str())
                .collect::<Vec<_>>(),
            ["session", "weekly"]
        );
        assert_eq!(snapshot.quotas[1].label, "Weekly (Unlimited)");
    }

    #[test]
    fn missing_invalid_and_rate_limited_keys_are_distinct() {
        let missing = MiniMaxProvider::with_dependencies(
            auth(None),
            MiniMaxClient::for_test(
                &test_http::serve_once(200, &[], REMAINS_BODY),
                Duration::from_secs(1),
            ),
        )
        .refresh()
        .unwrap_err();
        assert_eq!(missing.kind(), ProviderErrorKind::Authentication);

        for status in [401, 403] {
            let invalid = MiniMaxProvider::with_dependencies(
                auth(Some("bad-key")),
                MiniMaxClient::for_test(
                    &test_http::serve_once(status, &[], "{}"),
                    Duration::from_secs(1),
                ),
            )
            .refresh()
            .unwrap_err();
            assert_eq!(invalid.kind(), ProviderErrorKind::Authentication);
            assert!(!invalid.to_string().contains("bad-key"));
        }

        let rate_limited = MiniMaxProvider::with_dependencies(
            auth(Some("secret")),
            MiniMaxClient::for_test(
                &test_http::serve_once(429, &[], "{}"),
                Duration::from_secs(1),
            ),
        )
        .refresh()
        .unwrap_err();
        assert_eq!(rate_limited.kind(), ProviderErrorKind::RateLimited);
    }

    #[test]
    fn no_token_plan_is_a_permission_error() {
        let url = test_http::serve_once(
            200,
            &[],
            r#"{"base_resp":{"status_code":1001,"status_msg":"user has no token plan"}}"#,
        );
        let provider = MiniMaxProvider::with_dependencies(
            auth(Some("secret")),
            MiniMaxClient::for_test(&url, Duration::from_secs(1)),
        );
        let error = provider.refresh().unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::Permission);
    }

    #[test]
    fn definition_exposes_expected_identity_and_metrics() {
        let definition = definition(Site::Global);
        assert_eq!(definition.id, "minimax");
        assert_eq!(definition.display_name, "MiniMax");
        assert_eq!(
            definition
                .links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            ["Dashboard", "API Keys"]
        );
    }
}
