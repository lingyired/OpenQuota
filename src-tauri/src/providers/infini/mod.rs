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
    auth::InfiniAuthStore,
    client::{EndpointResponse, InfiniClient},
    mapper::{is_invalid_key, map_usage},
};

use super::{ProviderError, ProviderRequestContext, UsageProvider};

/// A Coding Plan is a request-counted subscription rather than a balance, so the
/// card reports the plan name the endpoint implies instead of a spend figure.
const PLAN_LABEL: &str = "Coding Plan";

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "infini".into(),
        display_name: "Infini".into(),
        short_name: "IN".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![ProviderLink::new(
            "Dashboard",
            "https://cloud.infini-ai.com/platform/ai",
        )],
        metrics: vec![
            MetricDefinition::quota(
                "infini.fiveHour",
                "Session (5h)",
                "fiveHour",
                true,
                true,
                MetricSection::AlwaysVisible,
                false,
                "5H",
            ),
            MetricDefinition::quota(
                "infini.sevenDay",
                "Weekly",
                "sevenDay",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "7D",
            ),
            MetricDefinition::quota(
                "infini.thirtyDay",
                "Monthly",
                "thirtyDay",
                false,
                true,
                MetricSection::OnDemand,
                false,
                "30D",
            ),
        ],
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum InfiniError {
    #[error("Add an Infini API key in Customize or set INFINI_API_KEY.")]
    MissingKey,
    #[error("The Infini API key is invalid. Check it at cloud.infini-ai.com.")]
    InvalidKey,
    #[error("Could not reach Infini. Check your internet connection.")]
    ConnectionFailed,
    #[error("Infini usage data is temporarily unavailable.")]
    InvalidResponse,
    #[error("Infini request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("The Infini API key could not be read or updated.")]
    CredentialStorage,
}

impl From<InfiniError> for ProviderError {
    fn from(error: InfiniError) -> Self {
        let kind = match error {
            InfiniError::MissingKey | InfiniError::InvalidKey => ProviderErrorKind::Authentication,
            InfiniError::ConnectionFailed => ProviderErrorKind::Network,
            InfiniError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            InfiniError::RequestFailed(401 | 403) => ProviderErrorKind::Authentication,
            InfiniError::RequestFailed(_) | InfiniError::InvalidResponse => {
                ProviderErrorKind::InvalidResponse
            }
            InfiniError::CredentialStorage => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::new(kind, error.to_string())
    }
}

pub struct InfiniProvider {
    auth: InfiniAuthStore,
    client: Arc<InfiniClient>,
}

impl InfiniProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            auth: InfiniAuthStore::new(),
            client: Arc::new(InfiniClient::new().map_err(ProviderError::from)?),
        })
    }

    #[cfg(test)]
    fn with_dependencies(auth: InfiniAuthStore, client: InfiniClient) -> Self {
        Self {
            auth,
            client: Arc::new(client),
        }
    }

    fn refresh_snapshot(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<ProviderSnapshot, ProviderError> {
        let response = required_response(self.client.fetch(context, api_key))?;
        if is_invalid_key(&response.body) {
            return Err(InfiniError::InvalidKey.into());
        }
        let quotas = map_usage(&response.body).map_err(ProviderError::from)?;
        Ok(ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "infini".into(),
            plan: Some(PLAN_LABEL.into()),
            quotas,
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        })
    }
}

impl UsageProvider for InfiniProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
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
            .ok_or_else(|| ProviderError::from(InfiniError::MissingKey))?;
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

fn required_response(
    response: Result<EndpointResponse, InfiniError>,
) -> Result<EndpointResponse, InfiniError> {
    let response = response?;
    if matches!(
        response.status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(InfiniError::InvalidKey);
    }
    if !response.status.is_success() {
        return Err(InfiniError::RequestFailed(response.status.as_u16()));
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

    use super::{auth::InfiniAuthStore, client::InfiniClient, definition, InfiniProvider};

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

    fn auth(key: Option<&str>) -> InfiniAuthStore {
        InfiniAuthStore::with_store(ApiKeyStore::with_backends(
            "infini",
            "INFINI_API_KEY",
            Arc::new(MemorySecrets::default()),
            Arc::new(Environment(
                key.map(|value| HashMap::from([("INFINI_API_KEY".to_owned(), value.to_owned())]))
                    .unwrap_or_default(),
            )),
        ))
    }

    const USAGE_BODY: &str = r#"{
        "5_hour": {"quota": 5000, "used": 1250, "remain": 3750},
        "7_day": {"quota": 30000, "used": 9000, "remain": 21000},
        "30_day": {"quota": 60000, "used": 0, "remain": 60000}
    }"#;

    fn provider(key: Option<&str>, status: u16, body: &str) -> InfiniProvider {
        let url = test_http::serve_once(status, &[], body);
        InfiniProvider::with_dependencies(
            auth(key),
            InfiniClient::for_test(&url, Duration::from_secs(1)),
        )
    }

    #[test]
    fn refresh_maps_the_three_request_windows() {
        let snapshot = provider(Some("sk-cp-secret"), 200, USAGE_BODY)
            .refresh()
            .unwrap();

        assert_eq!(snapshot.provider_id, "infini");
        assert_eq!(snapshot.plan.as_deref(), Some("Coding Plan"));
        assert!(snapshot.value_metrics.is_empty());
        assert_eq!(
            snapshot
                .quotas
                .iter()
                .map(|quota| (quota.id.as_str(), quota.used_percent))
                .collect::<Vec<_>>(),
            [("fiveHour", 25.0), ("sevenDay", 30.0), ("thirtyDay", 0.0)]
        );
    }

    #[test]
    fn a_rejected_key_is_an_authentication_error_on_any_transport_status() {
        let rejected = r#"{"code":10021,"msg":"CodingPlan的api key不正确，请确认后再重试"}"#;
        for status in [200, 401] {
            let error = provider(Some("sk-wrong"), status, rejected)
                .refresh()
                .unwrap_err();
            assert_eq!(error.kind(), ProviderErrorKind::Authentication);
        }
    }

    #[test]
    fn missing_and_rate_limited_keys_are_distinct() {
        let missing = provider(None, 200, USAGE_BODY).refresh().unwrap_err();
        assert_eq!(missing.kind(), ProviderErrorKind::Authentication);
        assert!(missing.to_string().contains("INFINI_API_KEY"));

        let rate_limited = provider(Some("sk-cp-secret"), 429, "{}")
            .refresh()
            .unwrap_err();
        assert_eq!(rate_limited.kind(), ProviderErrorKind::RateLimited);
    }

    #[test]
    fn an_unexpected_body_is_not_reported_as_a_bad_key() {
        let error = provider(Some("sk-cp-secret"), 200, r#"{"detail":"maintenance"}"#)
            .refresh()
            .unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::InvalidResponse);
    }

    #[test]
    fn definition_exposes_expected_identity_and_metrics() {
        let definition = definition();
        assert_eq!(definition.id, "infini");
        assert_eq!(definition.display_name, "Infini");
        assert!(!definition.fallback_enabled);
        assert_eq!(
            definition
                .metrics
                .iter()
                .map(|metric| metric.id.as_str())
                .collect::<Vec<_>>(),
            ["infini.fiveHour", "infini.sevenDay", "infini.thirtyDay"]
        );
        assert_eq!(
            definition
                .links
                .iter()
                .map(|link| link.url.as_str())
                .collect::<Vec<_>>(),
            ["https://cloud.infini-ai.com/platform/ai"]
        );
    }

    #[test]
    fn api_key_capability_delegates_without_exposing_the_secret() {
        let provider = provider(Some("environment"), 200, USAGE_BODY);
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
