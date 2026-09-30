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
    auth::ZaiAuthStore,
    client::{ZaiClient, ZaiResponse},
    mapper::{is_no_coding_plan, map_usage},
};

use super::{ProviderError, ProviderRequestContext, UsageProvider};

/// Z.ai ships the same GLM Coding Plan backend under two brands: `z.ai` for the
/// international site and 智谱 BigModel (`open.bigmodel.cn`) for mainland China.
/// Both endpoints answer the same paths with the same response shapes, so one
/// implementation serves either site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    Global,
    Cn,
}

impl Site {
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Global => "zai",
            Self::Cn => "zai-cn",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Global => "Z.ai",
            Self::Cn => "Z.ai CN",
        }
    }

    pub(crate) const fn short_name(self) -> &'static str {
        match self {
            Self::Global => "Z",
            Self::Cn => "ZC",
        }
    }

    pub(crate) const fn environment_names(self) -> &'static [&'static str] {
        match self {
            Self::Global => &["ZAI_API_KEY", "GLM_API_KEY"],
            Self::Cn => &["ZAI_CN_API_KEY", "BIGMODEL_API_KEY", "ZHIPU_API_KEY"],
        }
    }

    pub(crate) const fn config_paths(self) -> &'static [&'static str] {
        match self {
            Self::Global => &["~/.config/quota01/zai.json", "~/.config/zai/key.json"],
            Self::Cn => &["~/.config/quota01/zai-cn.json"],
        }
    }

    pub(crate) const fn subscription_url(self) -> &'static str {
        match self {
            Self::Global => "https://api.z.ai/api/biz/subscription/list",
            Self::Cn => "https://open.bigmodel.cn/api/biz/subscription/list",
        }
    }

    pub(crate) const fn quota_url(self) -> &'static str {
        match self {
            Self::Global => "https://api.z.ai/api/monitor/usage/quota/limit",
            Self::Cn => "https://open.bigmodel.cn/api/monitor/usage/quota/limit",
        }
    }

    pub(crate) const fn missing_key_message(self) -> &'static str {
        match self {
            Self::Global => {
                "Add a Z.ai API key in Customize, set ZAI_API_KEY, or configure ~/.config/quota01/zai.json."
            }
            Self::Cn => {
                "Add a Z.ai CN API key in Customize, set ZAI_CN_API_KEY, or configure ~/.config/quota01/zai-cn.json."
            }
        }
    }

    pub(crate) const fn invalid_key_message(self) -> &'static str {
        match self {
            Self::Global => {
                "The Z.ai API key is invalid. Check it at z.ai/manage-apikey/apikey-list."
            }
            Self::Cn => {
                "The Z.ai CN API key is invalid. Check it at open.bigmodel.cn/user-center/apikeys."
            }
        }
    }

    pub(crate) const fn no_coding_plan_message(self) -> &'static str {
        match self {
            Self::Global => "No active GLM Coding Plan. Subscribe at z.ai/subscribe to view usage.",
            Self::Cn => {
                "No active GLM Coding Plan. Subscribe at bigmodel.cn/glm-coding to view usage."
            }
        }
    }

    fn links(self) -> Vec<ProviderLink> {
        match self {
            Self::Global => vec![
                ProviderLink::new(
                    "Dashboard",
                    "https://z.ai/manage-apikey/coding-plan/personal/my-plan",
                ),
                ProviderLink::new("API Keys", "https://z.ai/manage-apikey/apikey-list"),
            ],
            Self::Cn => vec![
                ProviderLink::new("Dashboard", "https://open.bigmodel.cn/user-center/usage"),
                ProviderLink::new("API Keys", "https://open.bigmodel.cn/user-center/apikeys"),
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
            MetricDefinition::quota(
                &format!("{}.webSearches", site.id()),
                "Web Searches",
                "webSearches",
                false,
                true,
                MetricSection::OnDemand,
                false,
                "Search",
            ),
        ],
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum ZaiError {
    #[error("{}", .0.missing_key_message())]
    MissingKey(Site),
    #[error("{}", .0.invalid_key_message())]
    InvalidKey(Site),
    #[error("Could not reach Z.ai. Check your internet connection.")]
    ConnectionFailed,
    #[error("Z.ai usage data is temporarily unavailable.")]
    InvalidResponse,
    #[error("Z.ai request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("{}", .0.no_coding_plan_message())]
    NoCodingPlan(Site),
    /// Carries the credential store's own message, because an unwritable vault
    /// and a missing vault key need different advice. The site is kept so the
    /// error still knows which account it belongs to.
    #[error("{1}")]
    CredentialStorage(Site, String),
}

impl From<ZaiError> for ProviderError {
    fn from(error: ZaiError) -> Self {
        let kind = match &error {
            ZaiError::MissingKey(_) | ZaiError::InvalidKey(_) => ProviderErrorKind::Authentication,
            ZaiError::ConnectionFailed => ProviderErrorKind::Network,
            ZaiError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            ZaiError::RequestFailed(401 | 403) => ProviderErrorKind::Authentication,
            ZaiError::NoCodingPlan(_) => ProviderErrorKind::Permission,
            ZaiError::RequestFailed(_) | ZaiError::InvalidResponse => {
                ProviderErrorKind::InvalidResponse
            }
            ZaiError::CredentialStorage(_, _) => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::from_display(kind, error)
    }
}

pub struct ZaiProvider {
    site: Site,
    auth: ZaiAuthStore,
    client: Arc<ZaiClient>,
}

impl ZaiProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Self::with_site(Site::Global)
    }

    fn with_site(site: Site) -> Result<Self, ProviderError> {
        Ok(Self {
            site,
            auth: ZaiAuthStore::new(site),
            client: Arc::new(ZaiClient::new(site).map_err(ProviderError::from)?),
        })
    }

    #[cfg(test)]
    fn with_dependencies(auth: ZaiAuthStore, client: ZaiClient) -> Self {
        Self {
            site: Site::Global,
            auth,
            client: Arc::new(client),
        }
    }

    fn refresh_snapshot(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<ProviderSnapshot, ProviderError> {
        let quota = required_response(self.site, self.client.fetch_quota(context, api_key))?;
        if is_no_coding_plan(&quota.body) {
            return Err(ZaiError::NoCodingPlan(self.site).into());
        }
        let subscription = self
            .client
            .fetch_subscription(context, api_key)
            .ok()
            .filter(|response| response.status.is_success());
        let mapped = map_usage(
            &quota.body,
            subscription.as_ref().map(|response| &response.body),
        )?;
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

impl UsageProvider for ZaiProvider {
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
            .ok_or_else(|| ProviderError::from(ZaiError::MissingKey(self.site)))?;
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

/// Mainland-China runtime for the 智谱 BigModel site. It shares every code path
/// with [`ZaiProvider`]; only the site (id, endpoints, credential sources) differs.
pub struct ZaiCnProvider(ZaiProvider);

impl ZaiCnProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self(ZaiProvider::with_site(Site::Cn)?))
    }
}

impl UsageProvider for ZaiCnProvider {
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
    response: Result<ZaiResponse, ZaiError>,
) -> Result<ZaiResponse, ZaiError> {
    let response = response?;
    if matches!(
        response.status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(ZaiError::InvalidKey(site));
    }
    if !response.status.is_success() {
        return Err(ZaiError::RequestFailed(response.status.as_u16()));
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
        models::{ApiKeyStatus, MetricSection, ProviderErrorKind, QuotaFormat},
        providers::{
            api_key::{ApiKeyStore, EnvironmentReader, SecretBackend, SecretBytes},
            test_http, UsageProvider,
        },
    };

    use super::{auth::ZaiAuthStore, client::ZaiClient, definition, Site, ZaiProvider};

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

    fn auth(key: Option<&str>) -> ZaiAuthStore {
        ZaiAuthStore::with_store(
            Site::Global,
            ApiKeyStore::with_backends(
                "zai",
                "ZAI_API_KEY",
                Arc::new(MemorySecrets::default()),
                Arc::new(Environment(
                    key.map(|value| HashMap::from([("ZAI_API_KEY".into(), value.into())]))
                        .unwrap_or_default(),
                )),
            ),
        )
    }

    fn provider(
        key: Option<&str>,
        quota_status: u16,
        quota_body: &str,
        subscription_status: u16,
        subscription_body: &str,
    ) -> ZaiProvider {
        let quota_url = test_http::serve_once(quota_status, &[], quota_body);
        let subscription_url = test_http::serve_once(subscription_status, &[], subscription_body);
        ZaiProvider::with_dependencies(
            auth(key),
            ZaiClient::for_test(&subscription_url, &quota_url, Duration::from_secs(1)),
        )
    }

    #[test]
    fn refresh_maps_required_quota_and_optional_subscription() {
        let snapshot = provider(
            Some("secret"),
            200,
            include_str!("fixtures/quota.json"),
            200,
            include_str!("fixtures/subscription.json"),
        )
        .refresh()
        .unwrap();

        assert_eq!(snapshot.plan.as_deref(), Some("GLM Coding Pro"));
        assert_eq!(
            snapshot
                .quotas
                .iter()
                .map(|quota| quota.id.as_str())
                .collect::<Vec<_>>(),
            ["session", "weekly", "webSearches"]
        );
        assert_eq!(snapshot.quotas[2].format, QuotaFormat::Count);
        assert_eq!(snapshot.quotas[2].unit.as_deref(), Some("searches"));
        assert!(snapshot.status_metrics.is_empty());
        assert!(snapshot.warnings.is_empty());
    }

    #[test]
    fn subscription_failure_does_not_blank_required_quota() {
        let snapshot = provider(
            Some("secret"),
            200,
            include_str!("fixtures/quota.json"),
            503,
            "{}",
        )
        .refresh()
        .unwrap();

        assert_eq!(snapshot.plan, None);
        assert_eq!(snapshot.quotas.len(), 3);
    }

    #[test]
    fn missing_invalid_and_rate_limited_keys_are_distinct() {
        let missing = provider(None, 200, "{}", 200, "{}").refresh().unwrap_err();
        assert_eq!(missing.kind(), ProviderErrorKind::Authentication);
        assert!(missing.to_string().contains("Add a Z.ai API key"));

        for status in [401, 403] {
            let invalid = provider(Some("bad-key"), status, "{}", 200, "{}")
                .refresh()
                .unwrap_err();
            assert_eq!(invalid.kind(), ProviderErrorKind::Authentication);
            assert!(invalid.to_string().contains("invalid"));
            assert!(!invalid.to_string().contains("bad-key"));
        }

        let rate_limited = provider(Some("secret"), 429, "{}", 200, "{}")
            .refresh()
            .unwrap_err();
        assert_eq!(rate_limited.kind(), ProviderErrorKind::RateLimited);
    }

    #[test]
    fn no_coding_plan_and_malformed_payloads_are_typed() {
        let no_plan = provider(
            Some("secret"),
            200,
            r#"{"code":500,"msg":"Current user has no coding plan","success":false}"#,
            200,
            include_str!("fixtures/subscription.json"),
        )
        .refresh()
        .unwrap_err();
        assert_eq!(no_plan.kind(), ProviderErrorKind::Permission);
        assert!(no_plan.to_string().contains("GLM Coding Plan"));

        let malformed = provider(
            Some("secret"),
            200,
            r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5}]}}"#,
            200,
            include_str!("fixtures/subscription.json"),
        )
        .refresh()
        .unwrap_err();
        assert_eq!(malformed.kind(), ProviderErrorKind::InvalidResponse);
    }

    #[test]
    fn transport_and_timeout_errors_do_not_expose_the_key() {
        let subscription_url = test_http::serve_once(200, &[], "{}");
        let provider = ZaiProvider::with_dependencies(
            auth(Some("super-secret-key")),
            ZaiClient::for_test(
                &subscription_url,
                "http://127.0.0.1:1",
                Duration::from_millis(100),
            ),
        );
        let error = provider.refresh().unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::Network);
        assert!(!error.to_string().contains("super-secret-key"));

        let delayed = test_http::serve_once_after(
            test_http::TIMEOUT_TEST_RESPONSE_DELAY,
            200,
            &[],
            include_str!("fixtures/quota.json"),
        );
        let subscription_url = test_http::serve_once(200, &[], "{}");
        let timeout = ZaiProvider::with_dependencies(
            auth(Some("another-secret")),
            ZaiClient::for_test(
                &subscription_url,
                &delayed,
                test_http::TIMEOUT_TEST_CLIENT_LIMIT,
            ),
        )
        .refresh()
        .unwrap_err();
        assert_eq!(timeout.kind(), ProviderErrorKind::Network);
        assert!(!timeout.to_string().contains("another-secret"));
    }

    #[test]
    fn api_key_capability_uses_the_secure_vault_and_falls_back_to_environment() {
        let provider = provider(
            Some("environment-key"),
            200,
            include_str!("fixtures/quota.json"),
            200,
            include_str!("fixtures/subscription.json"),
        );
        assert_eq!(
            provider.api_key_status().unwrap().unwrap(),
            ApiKeyStatus::FromEnvironment
        );

        provider.save_api_key("saved-key").unwrap();
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

    #[test]
    fn definition_exposes_expected_links_and_default_metric_layout() {
        let mut definition = definition(Site::Global);
        crate::providers::normalize_default_pins(&mut definition.metrics);
        assert_eq!(definition.id, "zai");
        assert_eq!(definition.display_name, "Z.ai");
        assert_eq!(
            definition
                .links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            ["Dashboard", "API Keys"]
        );

        let metric = |id: &str| {
            definition
                .metrics
                .iter()
                .find(|metric| metric.id == id)
                .unwrap()
        };
        assert_eq!(
            metric("zai.session").default_section,
            MetricSection::AlwaysVisible
        );
        assert!(metric("zai.session").default_pinned);
        assert!(!metric("zai.session").source.session_window());
        assert_eq!(
            metric("zai.weekly").default_section,
            MetricSection::AlwaysVisible
        );
        assert!(metric("zai.weekly").default_pinned);
        assert_eq!(
            metric("zai.webSearches").default_section,
            MetricSection::OnDemand
        );
        assert!(!metric("zai.webSearches").default_pinned);
    }
}
