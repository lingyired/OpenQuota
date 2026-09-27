mod auth;
mod client;
mod mapper;

use std::sync::Arc;

use chrono::Utc;
use thiserror::Error;

use crate::models::{
    ApiKeyStatus, MetricDefinition, MetricSection, ProviderDefinition, ProviderErrorKind,
    ProviderLink, ProviderSnapshot, UsageHistory,
};

use self::{auth::OpenCodeAuthStore, client::OpenCodeClient, mapper::map_go_usage};

use super::{ProviderError, ProviderRequestContext, UsageProvider};

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "opencode".into(),
        display_name: "OpenCode Go".into(),
        short_name: "OC".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![ProviderLink::new("Dashboard", "https://opencode.ai/auth")],
        // OpenCode Go exposes three rolling subscription windows (rolling 5h / weekly / monthly).
        // The registry stars the first two pinnable metrics, so the default menu-bar pair is
        // Session and Weekly; Monthly stays one click away in Customize.
        metrics: vec![
            MetricDefinition::quota(
                "opencode.session",
                "Session",
                "session",
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                "S",
            )
            .with_floored_percent(),
            MetricDefinition::quota(
                "opencode.weekly",
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "W",
            )
            .with_floored_percent(),
            MetricDefinition::quota(
                "opencode.monthly",
                "Monthly",
                "monthly",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "M",
            )
            .with_floored_percent(),
        ],
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenCodeError {
    #[error("Add an OpenCode Go API key in Customize or set OPENCODE_GO_API_KEY.")]
    MissingKey,
    #[error("The OpenCode Go API key is invalid. Check it at opencode.ai.")]
    InvalidKey,
    #[error("OpenCode Go subscription required.")]
    GoSubscriptionRequired,
    #[error("Could not reach OpenCode Go. Check your internet connection.")]
    ConnectionFailed,
    #[error("OpenCode Go returned an invalid usage response.")]
    InvalidResponse,
    #[error("OpenCode Go usage request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("The OpenCode Go API key could not be read or updated.")]
    CredentialStorage,
}

impl From<OpenCodeError> for ProviderError {
    fn from(error: OpenCodeError) -> Self {
        let kind = match error {
            OpenCodeError::MissingKey | OpenCodeError::InvalidKey => {
                ProviderErrorKind::Authentication
            }
            OpenCodeError::GoSubscriptionRequired => ProviderErrorKind::Permission,
            OpenCodeError::ConnectionFailed => ProviderErrorKind::Network,
            OpenCodeError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            OpenCodeError::RequestFailed(500..=599) => ProviderErrorKind::Network,
            OpenCodeError::InvalidResponse | OpenCodeError::RequestFailed(_) => {
                ProviderErrorKind::InvalidResponse
            }
            OpenCodeError::CredentialStorage => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::new(kind, error.to_string())
    }
}

pub struct OpenCodeProvider {
    auth: OpenCodeAuthStore,
    client: OpenCodeClient,
}

impl OpenCodeProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            auth: OpenCodeAuthStore::new(),
            client: OpenCodeClient::new().map_err(ProviderError::from)?,
        })
    }

    #[cfg(test)]
    fn with_dependencies(auth: OpenCodeAuthStore, client: OpenCodeClient) -> Self {
        Self { auth, client }
    }

    fn refresh_snapshot(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<ProviderSnapshot, OpenCodeError> {
        let response = self.client.fetch_go_usage(context, api_key)?;
        let quotas = map_go_usage(response)?;
        Ok(ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "opencode".into(),
            plan: Some("Go".into()),
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

impl UsageProvider for OpenCodeProvider {
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
            .ok_or_else(|| ProviderError::from(OpenCodeError::MissingKey))?;
        self.refresh_snapshot(context, api_key.as_str())
            .map_err(ProviderError::from)
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
