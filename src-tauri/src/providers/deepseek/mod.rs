mod auth;
mod client;
mod mapper;

use std::sync::Arc;

use chrono::{Local, Utc};
use reqwest::StatusCode;
use thiserror::Error;

use crate::models::{
    ApiKeyStatus, MetricDefinition, MetricSection, ProviderDefinition, ProviderErrorKind,
    ProviderLink, ProviderSnapshot, UsageHistory, ValueMetric,
};

use self::{
    auth::DeepSeekAuthStore,
    client::DeepSeekClient,
    mapper::{map_summary, map_today_cost, DeepSeekMapError},
};

use super::{ProviderError, UsageProvider, WebviewAuth, WebviewCredentialSource};

const LOGIN_URL: &str = "https://platform.deepseek.com/sign_in";
const USER_TOKEN_STORAGE_KEY: &str = "userToken";
const LOGIN_WINDOW: &str = "deepseek-login";

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "deepseek".into(),
        display_name: "DeepSeek".into(),
        short_name: "DS".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![ProviderLink::new(
            "Dashboard",
            "https://platform.deepseek.com/usage",
        )],
        // The registry stars the first two pinnable metrics, so the default menu bar pair is
        // Balance and Today Spend; Total Spend stays one click away in Customize.
        metrics: vec![
            MetricDefinition::value(
                "deepseek.balance",
                "Balance",
                "balance",
                true,
                MetricSection::AlwaysVisible,
                true,
                "B",
                None,
            ),
            MetricDefinition::value(
                "deepseek.todaySpend",
                "Today Spend",
                "todaySpend",
                true,
                MetricSection::AlwaysVisible,
                true,
                "D",
                None,
            ),
            MetricDefinition::value(
                "deepseek.totalSpend",
                "Total Spend",
                "totalSpend",
                true,
                MetricSection::AlwaysVisible,
                false,
                "T",
                None,
            ),
        ],
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum DeepSeekError {
    #[error("Sign in to DeepSeek to view usage.")]
    MissingToken,
    #[error("Your DeepSeek session expired. Sign in again.")]
    InvalidToken,
    #[error("Could not reach DeepSeek. Check your internet connection.")]
    ConnectionFailed,
    #[error("DeepSeek usage data is temporarily unavailable.")]
    InvalidResponse,
    #[error("DeepSeek request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("The DeepSeek session could not be read or updated.")]
    CredentialStorage,
}

impl From<DeepSeekError> for ProviderError {
    fn from(error: DeepSeekError) -> Self {
        let kind = match error {
            DeepSeekError::MissingToken | DeepSeekError::InvalidToken => {
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

    fn refresh_snapshot(&self, user_token: &str) -> Result<ProviderSnapshot, ProviderError> {
        let response = self.client.fetch_summary(user_token)?;
        if matches!(
            response.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(DeepSeekError::InvalidToken.into());
        }
        if !response.status.is_success() {
            return Err(DeepSeekError::RequestFailed(response.status.as_u16()).into());
        }
        let mut mapped = map_summary(&response.body).map_err(map_error)?;

        let now = Local::now();
        let timezone = now.offset().local_minus_utc();
        let start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .ok_or(DeepSeekError::InvalidResponse)?
            .and_utc()
            .timestamp()
            - i64::from(timezone);
        let cost_response =
            self.client
                .fetch_today_cost(user_token, start, start + 86_400, timezone)?;
        if matches!(
            cost_response.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(DeepSeekError::InvalidToken.into());
        }
        if !cost_response.status.is_success() {
            return Err(DeepSeekError::RequestFailed(cost_response.status.as_u16()).into());
        }
        let today_spend =
            map_today_cost(&cost_response.body, &mapped.currencies).map_err(map_error)?;
        mapped.values.push(ValueMetric {
            id: "todaySpend".into(),
            label: "Today Spend".into(),
            values: today_spend,
            expiries_at: Vec::new(),
        });
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
        let user_token = self
            .auth
            .load()
            .map_err(ProviderError::from)?
            .ok_or_else(|| ProviderError::from(DeepSeekError::MissingToken))?;
        self.refresh_snapshot(user_token.as_str())
    }

    fn webview_auth(&self) -> Option<WebviewAuth> {
        Some(WebviewAuth {
            login_url: LOGIN_URL.into(),
            credential: WebviewCredentialSource::LocalStorage {
                key: USER_TOKEN_STORAGE_KEY.into(),
            },
            window_label: LOGIN_WINDOW.into(),
        })
    }

    fn session_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
        Some(self.auth.status().map_err(ProviderError::from))
    }

    fn save_session(&self, value: &str) -> Result<(), ProviderError> {
        self.auth.save(value).map_err(ProviderError::from)
    }

    fn delete_session(&self) -> Result<(), ProviderError> {
        self.auth.delete().map_err(ProviderError::from)
    }
}

fn map_error(error: DeepSeekMapError) -> ProviderError {
    match error {
        DeepSeekMapError::Authentication => DeepSeekError::InvalidToken.into(),
        DeepSeekMapError::InvalidResponse => DeepSeekError::InvalidResponse.into(),
    }
}

#[cfg(test)]
mod tests;
