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

use self::{auth::CommandCodeAuthStore, client::CommandCodeClient, mapper::map_billing};

use super::{ProviderError, ProviderRequestContext, UsageProvider};

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "commandcode".into(),
        display_name: "CommandCode".into(),
        short_name: "CC".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![ProviderLink::new(
            "Dashboard",
            "https://commandcode.ai/dashboard",
        )],
        // CommandCode exposes a monthly credit grant plus rolling rate-limit
        // windows (5h / weekly) that only appear while a limit is active. The
        // registry stars the first two pinnable metrics, so the default
        // menu-bar pair is Monthly and Session; Weekly stays one click away in
        // Customize.
        metrics: vec![
            MetricDefinition::quota(
                "commandcode.monthly",
                "Monthly",
                "monthly",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "M",
            ),
            MetricDefinition::quota(
                "commandcode.session",
                "Session (5h)",
                "session",
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                "5H",
            ),
            MetricDefinition::quota(
                "commandcode.weekly",
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                "W",
            ),
        ],
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum CommandCodeError {
    #[error("Add a CommandCode API key in Customize or set COMMANDCODE_API_KEY.")]
    MissingKey,
    #[error("The CommandCode API key is invalid. Check it at commandcode.ai.")]
    InvalidKey,
    #[error("Could not reach CommandCode. Check your internet connection.")]
    ConnectionFailed,
    #[error("CommandCode returned an invalid billing response.")]
    InvalidResponse,
    #[error("CommandCode billing request failed (HTTP {0}).")]
    RequestFailed(u16),
    /// Carries the credential store's own message, because an unwritable vault
    /// and a missing vault key need different advice.
    #[error("{0}")]
    CredentialStorage(String),
}

impl From<CommandCodeError> for ProviderError {
    fn from(error: CommandCodeError) -> Self {
        let kind = match &error {
            CommandCodeError::MissingKey | CommandCodeError::InvalidKey => {
                ProviderErrorKind::Authentication
            }
            CommandCodeError::ConnectionFailed => ProviderErrorKind::Network,
            CommandCodeError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            CommandCodeError::InvalidResponse | CommandCodeError::RequestFailed(_) => {
                ProviderErrorKind::InvalidResponse
            }
            CommandCodeError::CredentialStorage(_) => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::from_display(kind, error)
    }
}

pub struct CommandCodeProvider {
    auth: CommandCodeAuthStore,
    client: CommandCodeClient,
}

impl CommandCodeProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            auth: CommandCodeAuthStore::new(),
            client: CommandCodeClient::new().map_err(ProviderError::from)?,
        })
    }

    #[cfg(test)]
    fn with_dependencies(auth: CommandCodeAuthStore, client: CommandCodeClient) -> Self {
        Self { auth, client }
    }

    fn refresh_snapshot(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<ProviderSnapshot, CommandCodeError> {
        // The subscription is optional: it names the plan (Go / GOAT / Pro /
        // Max / Ultra) and dates the monthly reset, but a failure there must
        // not sink the credit report.
        let credits = self.client.fetch_credits(context, api_key)?;
        let subscription = self.client.fetch_subscription(context, api_key).ok();
        let billing = map_billing(credits, subscription)?;
        Ok(ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "commandcode".into(),
            plan: billing.plan,
            quotas: billing.quotas,
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        })
    }
}

impl UsageProvider for CommandCodeProvider {
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
            .ok_or_else(|| ProviderError::from(CommandCodeError::MissingKey))?;
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
