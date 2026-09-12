mod auth;
mod client;
mod installation;
mod mapper;

use std::sync::Arc;

use chrono::Utc;
use reqwest::StatusCode;
use thiserror::Error;

use crate::models::{
    ApiKeyStatus, MetricDefinition, MetricSection, MetricSource, MetricValue, MetricValueKind,
    ProviderDefinition, ProviderErrorKind, ProviderLink, ProviderSnapshot, UsageHistory,
    ValueMetric,
};

use self::{
    auth::TraeAuthStore,
    client::TraeClient,
    mapper::{map_entitlement, map_token},
};

use super::{ProviderError, UsageProvider, WebviewAuth};

const LOGIN_URL: &str = "https://www.trae.cn/account-setting#usage";
const SESSION_COOKIE: &str = "X-Cloudide-Session";
const LOGIN_WINDOW: &str = "trae-cn-login";

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "trae-cn".into(),
        display_name: "TraeWork CN".into(),
        short_name: "TR".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![ProviderLink::new(
            "Dashboard",
            "https://www.trae.cn/account-setting#usage",
        )],
        metrics: vec![
            MetricDefinition::quota(
                "trae-cn.credits",
                "Credits",
                "credits",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "C",
            ),
            MetricDefinition::value(
                "trae-cn.workCredits",
                "Work 专属积分",
                "workCredits",
                true,
                MetricSection::AlwaysVisible,
                false,
                "W",
                None,
            ),
            MetricDefinition::value(
                "trae-cn.generalCredits",
                "不含 Work 总积分",
                "generalCredits",
                true,
                MetricSection::OnDemand,
                false,
                "G",
                None,
            ),
            MetricDefinition::value(
                "trae-cn.nearestExpiring",
                "近期到期的积分包",
                "nearestExpiring",
                true,
                MetricSection::AlwaysVisible,
                false,
                "近",
                None,
            ),
            MetricDefinition::new(
                "trae-cn.creditPackages",
                "可用积分包",
                MetricSource::CreditPackages,
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                None,
                None,
            ),
            MetricDefinition::status(
                "trae-cn.status",
                "Status",
                "status",
                true,
                MetricSection::OnDemand,
                false,
                "S",
            ),
        ],
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum TraeError {
    #[error("Sign in to TraeWork CN to view usage.")]
    SessionMissing,
    #[error("Your TraeWork CN session expired. Sign in again.")]
    SessionExpired,
    #[error("Could not reach TraeWork CN. Check your internet connection.")]
    ConnectionFailed,
    #[error("TraeWork CN usage data is temporarily unavailable.")]
    InvalidResponse,
    #[error("TraeWork CN request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("The TraeWork CN session could not be read or updated.")]
    CredentialStorage,
}

impl From<TraeError> for ProviderError {
    fn from(error: TraeError) -> Self {
        let kind = match error {
            TraeError::SessionMissing | TraeError::SessionExpired => {
                ProviderErrorKind::Authentication
            }
            TraeError::ConnectionFailed => ProviderErrorKind::Network,
            TraeError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            TraeError::RequestFailed(401 | 403) => ProviderErrorKind::Authentication,
            TraeError::RequestFailed(_) | TraeError::InvalidResponse => {
                ProviderErrorKind::InvalidResponse
            }
            TraeError::CredentialStorage => ProviderErrorKind::CredentialStorage,
        };
        ProviderError::new(kind, error.to_string())
    }
}

pub struct TraeProvider {
    auth: TraeAuthStore,
    client: Arc<TraeClient>,
}

impl TraeProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            auth: TraeAuthStore::new(),
            client: Arc::new(TraeClient::new().map_err(ProviderError::from)?),
        })
    }

    #[cfg(test)]
    fn with_dependencies(auth: crate::providers::api_key::ApiKeyStore, client: TraeClient) -> Self {
        Self {
            auth: TraeAuthStore::with_store(auth),
            client: Arc::new(client),
        }
    }

    fn refresh_snapshot(&self, session: &str) -> Result<ProviderSnapshot, ProviderError> {
        let token_response = self.client.exchange_token(session)?;
        if matches!(
            token_response.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(TraeError::SessionExpired.into());
        }
        if !token_response.status.is_success() {
            return Err(TraeError::RequestFailed(token_response.status.as_u16()).into());
        }
        let token = map_token(&token_response.body).ok_or(TraeError::InvalidResponse)?;

        let credits_response = self.client.fetch_credits(&token.token)?;
        if matches!(
            credits_response.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(TraeError::SessionExpired.into());
        }
        if !credits_response.status.is_success() {
            return Err(TraeError::RequestFailed(credits_response.status.as_u16()).into());
        }
        let mapped = map_entitlement(&credits_response.body, Utc::now())
            .ok_or(TraeError::InvalidResponse)?;
        let nearest_number = mapped
            .nearest_expiring
            .as_ref()
            .map_or(0.0, |package| package.remaining);
        let nearest_expiries = mapped
            .nearest_expiring
            .as_ref()
            .and_then(|package| package.expires_at)
            .into_iter()
            .collect();
        let value_metrics = vec![
            ValueMetric {
                id: "workCredits".into(),
                label: "Work 专属积分".into(),
                values: vec![MetricValue {
                    number: mapped.work_remaining,
                    kind: MetricValueKind::Count,
                    label: None,
                    estimated: false,
                }],
                expiries_at: Vec::new(),
            },
            ValueMetric {
                id: "generalCredits".into(),
                label: "不含 Work 总积分".into(),
                values: vec![MetricValue {
                    number: mapped.general_remaining,
                    kind: MetricValueKind::Count,
                    label: None,
                    estimated: false,
                }],
                expiries_at: Vec::new(),
            },
            ValueMetric {
                id: "nearestExpiring".into(),
                label: "近期到期的积分包".into(),
                values: vec![MetricValue {
                    number: nearest_number,
                    kind: MetricValueKind::Count,
                    label: None,
                    estimated: false,
                }],
                expiries_at: nearest_expiries,
            },
        ];
        Ok(ProviderSnapshot {
            provider_id: "trae-cn".into(),
            plan: mapped.plan,
            quotas: mapped.quota.into_iter().collect(),
            credit_packages: mapped.packages,
            value_metrics,
            status_metrics: mapped.status.into_iter().collect(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        })
    }
}

impl UsageProvider for TraeProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    fn has_local_credentials(&self) -> bool {
        self.auth.has_credentials()
    }

    fn has_local_installation(&self) -> bool {
        installation::is_installed()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        let session = self
            .auth
            .load()
            .map_err(ProviderError::from)?
            .ok_or_else(|| ProviderError::from(TraeError::SessionMissing))?;
        self.refresh_snapshot(session.as_str())
    }

    fn webview_auth(&self) -> Option<WebviewAuth> {
        Some(WebviewAuth {
            login_url: LOGIN_URL.into(),
            cookie_name: SESSION_COOKIE.into(),
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

#[cfg(test)]
mod tests;
