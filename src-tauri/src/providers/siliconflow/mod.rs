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
    auth::SiliconFlowAuthStore,
    client::{EndpointResponse, SiliconFlowClient},
    mapper::{map_usage, BalanceMetrics},
};

use super::{ProviderError, ProviderRequestContext, UsageProvider};

/// SiliconFlow serves the same user-info endpoint from the international site
/// (`api.siliconflow.com`) and the mainland-China site (`api.siliconflow.cn`).
/// The two consoles hold separate accounts and API keys, so each site is
/// registered as its own provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    Global,
    Cn,
}

impl Site {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Global => "siliconflow",
            Self::Cn => "siliconflow-cn",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Global => "SiliconFlow",
            Self::Cn => "SiliconFlow CN",
        }
    }

    pub(crate) const fn short_name(self) -> &'static str {
        match self {
            Self::Global => "SF",
            Self::Cn => "SFC",
        }
    }

    pub(crate) const fn environment_names(self) -> &'static [&'static str] {
        match self {
            Self::Global => &["SILICONFLOW_API_KEY"],
            Self::Cn => &["SILICONFLOW_CN_API_KEY"],
        }
    }

    pub(crate) const fn config_paths(self) -> &'static [&'static str] {
        match self {
            Self::Global => &["~/.config/quota01/siliconflow.json"],
            Self::Cn => &["~/.config/quota01/siliconflow-cn.json"],
        }
    }

    pub(crate) const fn user_info_url(self) -> &'static str {
        match self {
            Self::Global => "https://api.siliconflow.com/v1/user/info",
            Self::Cn => "https://api.siliconflow.cn/v1/user/info",
        }
    }

    /// Balances are quoted in the site's own settlement currency.
    pub(crate) const fn currency(self) -> &'static str {
        match self {
            Self::Global => "USD",
            Self::Cn => "CNY",
        }
    }

    pub(crate) const fn missing_key_message(self) -> &'static str {
        match self {
            Self::Global => "Add a SiliconFlow API key in Customize or set SILICONFLOW_API_KEY.",
            Self::Cn => "Add a SiliconFlow CN API key in Customize or set SILICONFLOW_CN_API_KEY.",
        }
    }

    pub(crate) const fn invalid_key_message(self) -> &'static str {
        match self {
            Self::Global => {
                "The SiliconFlow API key is invalid. Check it at cloud.siliconflow.com/account/ak."
            }
            Self::Cn => {
                "The SiliconFlow CN API key is invalid. Check it at cloud.siliconflow.cn/account/ak."
            }
        }
    }

    fn links(self) -> Vec<ProviderLink> {
        match self {
            Self::Global => vec![
                ProviderLink::new("Dashboard", "https://cloud.siliconflow.com/account/balance"),
                ProviderLink::new("API Keys", "https://cloud.siliconflow.com/account/ak"),
            ],
            Self::Cn => vec![
                ProviderLink::new("Dashboard", "https://cloud.siliconflow.cn/account/balance"),
                ProviderLink::new("API Keys", "https://cloud.siliconflow.cn/account/ak"),
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
            MetricDefinition::value(
                &format!("{}.balance", site.id()),
                "Balance",
                "balance",
                true,
                MetricSection::AlwaysVisible,
                false,
                "B",
                None,
            ),
            MetricDefinition::value(
                &format!("{}.granted", site.id()),
                "Granted Balance",
                "granted",
                true,
                MetricSection::AlwaysVisible,
                false,
                "G",
                None,
            ),
            MetricDefinition::value(
                &format!("{}.recharged", site.id()),
                "Recharged Balance",
                "recharged",
                true,
                MetricSection::OnDemand,
                false,
                "R",
                None,
            ),
        ],
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum SiliconFlowError {
    #[error("{}", .0.missing_key_message())]
    MissingKey(Site),
    #[error("{}", .0.invalid_key_message())]
    InvalidKey(Site),
    #[error("Could not reach SiliconFlow. Check your internet connection.")]
    ConnectionFailed,
    #[error("SiliconFlow usage data is temporarily unavailable.")]
    InvalidResponse,
    #[error("SiliconFlow request failed (HTTP {0}).")]
    RequestFailed(u16),
    /// Carries the credential store's own message, because an unwritable vault
    /// and a missing vault key need different advice. The site is kept so the
    /// error still knows which account it belongs to.
    #[error("{1}")]
    CredentialStorage(Site, String),
}

impl From<SiliconFlowError> for ProviderError {
    fn from(error: SiliconFlowError) -> Self {
        let kind = match &error {
            SiliconFlowError::MissingKey(_) | SiliconFlowError::InvalidKey(_) => {
                ProviderErrorKind::Authentication
            }
            SiliconFlowError::ConnectionFailed => ProviderErrorKind::Network,
            SiliconFlowError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            SiliconFlowError::RequestFailed(401 | 403) => ProviderErrorKind::Authentication,
            SiliconFlowError::RequestFailed(_) | SiliconFlowError::InvalidResponse => {
                ProviderErrorKind::InvalidResponse
            }
            SiliconFlowError::CredentialStorage(_, _) => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::from_display(kind, error)
    }
}

pub struct SiliconFlowProvider {
    site: Site,
    auth: SiliconFlowAuthStore,
    client: Arc<SiliconFlowClient>,
}

impl SiliconFlowProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Self::with_site(Site::Global)
    }

    fn with_site(site: Site) -> Result<Self, ProviderError> {
        Ok(Self {
            site,
            auth: SiliconFlowAuthStore::new(site),
            client: Arc::new(SiliconFlowClient::new(site).map_err(ProviderError::from)?),
        })
    }

    #[cfg(test)]
    fn with_dependencies(
        site: Site,
        auth: SiliconFlowAuthStore,
        client: SiliconFlowClient,
    ) -> Self {
        Self {
            site,
            auth,
            client: Arc::new(client),
        }
    }

    fn refresh_snapshot(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<ProviderSnapshot, ProviderError> {
        let response = required_response(self.site, self.client.fetch(context, api_key))?;
        let BalanceMetrics {
            balance,
            granted,
            recharged,
        } = map_usage(self.site, &response.body)?;
        Ok(ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: self.site.id().into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: [balance, granted, recharged]
                .into_iter()
                .flatten()
                .collect(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        })
    }
}

impl UsageProvider for SiliconFlowProvider {
    fn definition(&self) -> ProviderDefinition {
        definition(self.site)
    }

    fn has_local_credentials(&self) -> bool {
        self.auth.has_local_credentials()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        self.refresh_with_context(&ProviderRequestContext::direct(Arc::default()))
    }

    fn refresh_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<ProviderSnapshot, ProviderError> {
        let api_key = self
            .auth
            .load()
            .map_err(ProviderError::from)?
            .ok_or_else(|| ProviderError::from(SiliconFlowError::MissingKey(self.site)))?;
        self.refresh_snapshot(context, api_key.as_str())
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

/// Mainland-China runtime for the `api.siliconflow.cn` account. It shares every
/// code path with [`SiliconFlowProvider`]; only the site (id, endpoint, currency,
/// credential sources) differs.
pub struct SiliconFlowCnProvider(SiliconFlowProvider);

impl SiliconFlowCnProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self(SiliconFlowProvider::with_site(Site::Cn)?))
    }
}

impl UsageProvider for SiliconFlowCnProvider {
    fn definition(&self) -> ProviderDefinition {
        definition(Site::Cn)
    }

    fn has_local_credentials(&self) -> bool {
        self.0.has_local_credentials()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        self.0.refresh()
    }

    fn refresh_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<ProviderSnapshot, ProviderError> {
        self.0.refresh_with_context(context)
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
    response: Result<EndpointResponse, SiliconFlowError>,
) -> Result<EndpointResponse, SiliconFlowError> {
    let response = response?;
    if matches!(
        response.status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(SiliconFlowError::InvalidKey(site));
    }
    if !response.status.is_success() {
        return Err(SiliconFlowError::RequestFailed(response.status.as_u16()));
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
        models::{ApiKeyStatus, ProviderErrorKind},
        providers::{api_key::*, test_http, UsageProvider},
    };

    use super::{
        auth::SiliconFlowAuthStore, client::SiliconFlowClient, definition, SiliconFlowCnProvider,
        SiliconFlowProvider, Site,
    };

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

    fn auth(site: Site, key: Option<&str>) -> SiliconFlowAuthStore {
        let name = site.environment_names()[0];
        SiliconFlowAuthStore::with_store(
            site,
            ApiKeyStore::with_backends(
                site.id(),
                name,
                Arc::new(MemorySecrets::default()),
                Arc::new(Environment(
                    key.map(|value| HashMap::from([(name.to_owned(), value.to_owned())]))
                        .unwrap_or_default(),
                )),
            ),
        )
    }

    const SUCCESS_BODY: &str = r#"{"code":20000,"message":"OK","status":true,"data":{
        "id":"user-1","name":"ling","balance":"0.88","chargeBalance":"10.00",
        "totalBalance":"10.88","status":"normal"}}"#;

    fn provider(site: Site, key: Option<&str>, status: u16, body: &str) -> SiliconFlowProvider {
        let url = test_http::serve_once(status, &[], body);
        SiliconFlowProvider::with_dependencies(
            site,
            auth(site, key),
            SiliconFlowClient::for_test(&url, Duration::from_secs(1)),
        )
    }

    #[test]
    fn refresh_maps_every_balance_of_the_account() {
        let snapshot = provider(Site::Global, Some("secret"), 200, SUCCESS_BODY)
            .refresh()
            .unwrap();

        assert_eq!(snapshot.provider_id, "siliconflow");
        assert_eq!(snapshot.plan, None);
        assert!(snapshot.quotas.is_empty());
        assert_eq!(
            snapshot
                .value_metrics
                .iter()
                .map(|metric| (metric.id.as_str(), metric.values[0].number))
                .collect::<Vec<_>>(),
            [("balance", 10.88), ("granted", 0.88), ("recharged", 10.0)]
        );
    }

    #[test]
    fn the_china_runtime_reads_its_own_site_and_currency() {
        let snapshot = provider(Site::Cn, Some("secret"), 200, SUCCESS_BODY)
            .refresh()
            .unwrap();

        assert_eq!(snapshot.provider_id, "siliconflow-cn");
        assert_eq!(
            snapshot.value_metrics[0].values[0].label.as_deref(),
            Some("CNY")
        );
        assert_eq!(
            SiliconFlowCnProvider::new().unwrap().definition().id,
            "siliconflow-cn"
        );
    }

    #[test]
    fn a_rejected_key_inside_a_200_response_is_an_authentication_error() {
        let error = provider(
            Site::Global,
            Some("bad-key"),
            200,
            r#"{"code":30014,"data":null,"message":"Token is invalid."}"#,
        )
        .refresh()
        .unwrap_err();

        assert_eq!(error.kind(), ProviderErrorKind::Authentication);
        assert!(!error.to_string().contains("bad-key"));
    }

    #[test]
    fn missing_invalid_and_rate_limited_keys_are_distinct() {
        let missing = provider(Site::Cn, None, 200, SUCCESS_BODY)
            .refresh()
            .unwrap_err();
        assert_eq!(missing.kind(), ProviderErrorKind::Authentication);
        assert!(missing.to_string().contains("SILICONFLOW_CN_API_KEY"));

        let invalid = provider(Site::Global, Some("secret"), 401, "{}")
            .refresh()
            .unwrap_err();
        assert_eq!(invalid.kind(), ProviderErrorKind::Authentication);

        let rate_limited = provider(Site::Global, Some("secret"), 429, "{}")
            .refresh()
            .unwrap_err();
        assert_eq!(rate_limited.kind(), ProviderErrorKind::RateLimited);
    }

    #[test]
    fn definition_exposes_expected_identity_and_metrics() {
        let global = definition(Site::Global);
        assert_eq!(global.id, "siliconflow");
        assert_eq!(global.display_name, "SiliconFlow");
        assert!(!global.fallback_enabled);
        assert_eq!(
            global
                .metrics
                .iter()
                .map(|metric| metric.id.as_str())
                .collect::<Vec<_>>(),
            [
                "siliconflow.balance",
                "siliconflow.granted",
                "siliconflow.recharged"
            ]
        );
        assert_eq!(
            global
                .links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            ["Dashboard", "API Keys"]
        );
        assert_eq!(definition(Site::Cn).id, "siliconflow-cn");
        assert_eq!(definition(Site::Cn).display_name, "SiliconFlow CN");
    }

    #[test]
    fn api_key_capability_delegates_without_exposing_the_secret() {
        let provider = provider(Site::Global, Some("environment"), 200, SUCCESS_BODY);
        assert_eq!(
            provider.api_key_status().unwrap().unwrap(),
            ApiKeyStatus::FromEnvironment
        );
        provider.save_api_key("saved").unwrap();
        assert_eq!(
            provider.api_key_status().unwrap().unwrap(),
            ApiKeyStatus::OverrideActive
        );
        provider.delete_api_key().unwrap();
        assert_eq!(
            provider.api_key_status().unwrap().unwrap(),
            ApiKeyStatus::FromEnvironment
        );
    }
}
