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

use self::{auth::DeepSeekAuthStore, client::DeepSeekClient, mapper::map_balance};

use super::{ProviderError, UsageProvider};

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "deepseek".into(),
        display_name: "DeepSeek".into(),
        short_name: "DS".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![
            ProviderLink::new("Dashboard", "https://platform.deepseek.com/usage"),
            ProviderLink::new("API Keys", "https://platform.deepseek.com/api_keys"),
        ],
        metrics: vec![MetricDefinition::value(
            "deepseek.balance",
            "Balance",
            "balance",
            true,
            MetricSection::AlwaysVisible,
            true,
            "B",
            None,
        )],
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum DeepSeekError {
    #[error("Add a DeepSeek API key in Customize or set DEEPSEEK_API_KEY.")]
    MissingKey,
    #[error("The DeepSeek API key is invalid. Check it at platform.deepseek.com.")]
    InvalidKey,
    #[error("Could not reach DeepSeek. Check your internet connection.")]
    ConnectionFailed,
    #[error("DeepSeek usage data is temporarily unavailable.")]
    InvalidResponse,
    #[error("DeepSeek request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("The DeepSeek API key could not be read or updated.")]
    CredentialStorage,
}

impl From<DeepSeekError> for ProviderError {
    fn from(error: DeepSeekError) -> Self {
        let kind = match error {
            DeepSeekError::MissingKey | DeepSeekError::InvalidKey => {
                ProviderErrorKind::Authentication
            }
            DeepSeekError::ConnectionFailed => ProviderErrorKind::Network,
            DeepSeekError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            DeepSeekError::RequestFailed(401 | 403) => ProviderErrorKind::Authentication,
            DeepSeekError::RequestFailed(_) | DeepSeekError::InvalidResponse => {
                ProviderErrorKind::InvalidResponse
            }
            DeepSeekError::CredentialStorage => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::new(kind, error.to_string())
    }
}

pub struct DeepSeekProvider {
    auth: DeepSeekAuthStore,
    client: Arc<DeepSeekClient>,
}

impl DeepSeekProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            auth: DeepSeekAuthStore::new(),
            client: Arc::new(DeepSeekClient::new().map_err(ProviderError::from)?),
        })
    }

    #[cfg(test)]
    fn with_dependencies(
        auth: crate::providers::api_key::ApiKeyStore,
        client: DeepSeekClient,
    ) -> Self {
        Self {
            auth: DeepSeekAuthStore::with_store(auth),
            client: Arc::new(client),
        }
    }

    fn refresh_snapshot(&self, api_key: &str) -> Result<ProviderSnapshot, ProviderError> {
        let response = self.client.fetch_balance(api_key)?;
        if matches!(
            response.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(DeepSeekError::InvalidKey.into());
        }
        if !response.status.is_success() {
            return Err(DeepSeekError::RequestFailed(response.status.as_u16()).into());
        }
        let mapped = map_balance(&response.body).ok_or(DeepSeekError::InvalidResponse)?;
        Ok(ProviderSnapshot {
            provider_id: "deepseek".into(),
            plan: mapped.plan,
            quotas: Vec::new(),
            credit_packages: Vec::new(),
            value_metrics: mapped.values,
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        })
    }
}

impl UsageProvider for DeepSeekProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    fn has_local_credentials(&self) -> bool {
        self.auth.has_credentials()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        let api_key = self
            .auth
            .load()
            .map_err(ProviderError::from)?
            .ok_or_else(|| ProviderError::from(DeepSeekError::MissingKey))?;
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

#[cfg(test)]
mod tests;
