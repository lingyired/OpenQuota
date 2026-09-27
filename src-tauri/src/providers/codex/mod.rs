pub mod auth;
pub mod client;
pub mod local_usage;
pub mod mapper;
pub mod reset_claim;

use std::sync::Arc;

use chrono::Utc;
use reqwest::StatusCode;
use thiserror::Error;

use crate::{
    hashing::sha256_hex,
    models::{
        MetricDefinition, MetricSection, ProviderDefinition, ProviderLink, ProviderSnapshot,
        UsagePeriodSelection,
    },
    pricing::PricingStore,
    storage::Storage,
};

use self::{
    auth::CodexAuthState,
    client::{CodexClient, UsageResponse},
    local_usage::scan_local_usage,
    mapper::map_usage,
};
use crate::providers::{log_usage::scan_or_cached_usage, ProviderRequestContext};

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "codex".into(),
        display_name: "Codex".into(),
        short_name: "Cx".into(),
        fallback_enabled: true,
        local_usage_source_note: Some("From your Codex logs (estimated)".into()),
        links: vec![
            ProviderLink::new("Status", "https://status.openai.com/"),
            ProviderLink::new("Dashboard", "https://chatgpt.com/codex/settings/usage"),
        ],
        metrics: vec![
            MetricDefinition::quota(
                "codex.session",
                "Session",
                "session",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "S",
            ),
            MetricDefinition::quota(
                "codex.weekly",
                "Weekly",
                "weekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "W",
            ),
            MetricDefinition::quota(
                "codex.spark",
                "Spark",
                "spark",
                false,
                true,
                MetricSection::OnDemand,
                false,
                "Sp",
            ),
            MetricDefinition::quota(
                "codex.sparkWeekly",
                "Spark Weekly",
                "sparkWeekly",
                false,
                true,
                MetricSection::OnDemand,
                false,
                "SW",
            ),
            MetricDefinition::trend("codex.trend"),
            MetricDefinition::value(
                "codex.credits",
                "Extra Usage",
                "credits",
                true,
                MetricSection::OnDemand,
                false,
                "E",
                None,
            ),
            MetricDefinition::value(
                "codex.rateLimitResets",
                "Rate Limit Resets",
                "rateLimitResets",
                true,
                MetricSection::OnDemand,
                false,
                "R",
                Some("resets"),
            ),
            MetricDefinition::usage(
                "codex.today",
                "Today",
                UsagePeriodSelection::Today,
                MetricSection::OnDemand,
                "T",
            ),
            MetricDefinition::usage(
                "codex.yesterday",
                "Yesterday",
                UsagePeriodSelection::Yesterday,
                MetricSection::OnDemand,
                "Y",
            ),
            MetricDefinition::usage(
                "codex.last30",
                "Last 30 Days",
                UsagePeriodSelection::Last30Days,
                MetricSection::OnDemand,
                "M",
            ),
        ],
    }
}

#[derive(Debug, Error)]
pub enum CodexError {
    #[error("Not logged in. Run `codex` to authenticate.")]
    NotLoggedIn,
    #[error(
        "Subscription usage is unavailable for API-key-only logins. Sign in to Codex with ChatGPT."
    )]
    ApiKeyOnly,
    #[error("Your Codex session expired. Run `codex` to sign in again.")]
    SessionExpired,
    #[error("Codex credentials changed while refreshing. Run `codex` to sign in again.")]
    TokenConflict,
    #[error("Your Codex session was revoked. Run `codex` to sign in again.")]
    TokenRevoked,
    #[error("Your Codex access token expired. Run `codex` to sign in again.")]
    TokenExpired,
    #[error("Codex auth data is invalid. Run `codex` to sign in again.")]
    InvalidAuth,
    #[error("The Codex account changed while usage was refreshing. Refresh again.")]
    AccountChanged,
    #[error("Refreshed Codex credentials could not be saved.")]
    AuthWrite,
    #[error("Codex usage request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("Codex returned an invalid usage response.")]
    InvalidResponse,
    #[error("Could not connect to Codex. Check your internet connection.")]
    ConnectionFailed,
    #[error("Local Codex usage logs could not be processed.")]
    LocalUsage,
    #[error("Quota01 cache is unavailable.")]
    Storage,
}

impl From<crate::storage::StorageError> for CodexError {
    fn from(_: crate::storage::StorageError) -> Self {
        Self::Storage
    }
}

pub struct CodexProvider {
    account_identity: Option<String>,
    storage: Arc<Storage>,
    pricing: Arc<PricingStore>,
    client: CodexClient,
}

impl CodexProvider {
    pub fn new(storage: Arc<Storage>, pricing: Arc<PricingStore>) -> Result<Self, CodexError> {
        let account_identity = CodexAuthState::observed_account_identity()
            .map(|identity| account_identity_key(&identity));
        if let Some(identity) = account_identity.as_deref() {
            crate::providers::remember_default_account(&storage, "codex", identity)?;
        }
        Ok(Self {
            account_identity,
            storage,
            pricing,
            client: CodexClient::new()?,
        })
    }

    pub fn refresh(&self) -> Result<ProviderSnapshot, CodexError> {
        self.refresh_with_identity(&ProviderRequestContext::direct(Arc::default()))
            .map(|(snapshot, _)| snapshot)
    }

    fn refresh_with_identity(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<(ProviderSnapshot, Option<String>), CodexError> {
        let now = Utc::now();
        let candidates = CodexAuthState::load_candidates()?;
        crate::app_debug!(
            "auth:codex",
            "credential candidates loaded ({})",
            candidates.len()
        );
        let mut last_auth_error = None;
        for mut auth in candidates {
            let identity = auth
                .account_identity()
                .map(|identity| account_identity_key(&identity));
            match self.refresh_candidate(context, &mut auth, now, identity.as_deref()) {
                Ok(snapshot) => return Ok((snapshot, identity)),
                Err(
                    error @ (CodexError::SessionExpired
                    | CodexError::TokenConflict
                    | CodexError::TokenRevoked
                    | CodexError::TokenExpired),
                ) => last_auth_error = Some(error),
                Err(error) => return Err(error),
            }
        }
        Err(last_auth_error.unwrap_or(CodexError::NotLoggedIn))
    }

    fn ensure_candidate_identity(
        auth: &CodexAuthState,
        expected: Option<&str>,
    ) -> Result<(), CodexError> {
        let observed = auth
            .account_identity()
            .map(|identity| account_identity_key(&identity));
        validate_account_identity(expected, observed.as_deref())
    }

    fn ensure_candidate_source_current(
        auth: &CodexAuthState,
        expected: Option<&str>,
    ) -> Result<(), CodexError> {
        let current = auth.reload().map_err(|_| CodexError::AccountChanged)?;
        Self::ensure_candidate_identity(&current, expected)
    }

    fn fetch_remote_usage_data(
        &self,
        context: &ProviderRequestContext,
        auth: &CodexAuthState,
    ) -> Result<(UsageResponse, Option<UsageResponse>), CodexError> {
        let response = self.client.fetch_usage_with_context(
            context,
            &auth.access_token,
            auth.account_id.as_deref(),
        )?;
        let reset_credits = if response.status.is_success() {
            self.client
                .fetch_reset_credits_with_context(
                    context,
                    &auth.access_token,
                    auth.account_id.as_deref(),
                )
                .ok()
        } else {
            None
        };
        Ok((response, reset_credits))
    }

    fn refresh_candidate(
        &self,
        context: &ProviderRequestContext,
        auth: &mut CodexAuthState,
        now: chrono::DateTime<Utc>,
        account_identity: Option<&str>,
    ) -> Result<ProviderSnapshot, CodexError> {
        let mut warnings = Vec::new();

        Self::ensure_candidate_identity(auth, account_identity)?;

        if auth.needs_refresh(now) {
            if let Ok(live) = auth.reload() {
                Self::ensure_candidate_identity(&live, account_identity)?;
                *auth = live;
            }
        }
        if auth.needs_refresh(now) {
            self.refresh_access_token(context, auth, now, &mut warnings)?;
            Self::ensure_candidate_identity(auth, account_identity)?;
        }

        let (mut response, mut reset_credits) = self.fetch_remote_usage_data(context, auth)?;
        if matches!(
            response.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            self.refresh_access_token(context, auth, now, &mut warnings)?;
            Self::ensure_candidate_identity(auth, account_identity)?;
            (response, reset_credits) = self.fetch_remote_usage_data(context, auth)?;
        }
        let mapped = map_usage(&response, reset_credits.as_ref(), now)?;
        let pricing = self.pricing.current();
        let usage = scan_or_cached_usage(
            &self.storage,
            "codex",
            account_identity
                .map(crate::providers::CacheIdentity::Resolved)
                .unwrap_or(crate::providers::CacheIdentity::Unresolved),
            "Codex",
            || scan_local_usage(&self.storage, now, &pricing),
            &mut warnings,
        );
        Self::ensure_candidate_source_current(auth, account_identity)?;
        Ok(ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "codex".into(),
            plan: mapped.plan,
            quotas: mapped.quotas,
            value_metrics: mapped.value_metrics,
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage,
            warnings,
            refreshed_at: now,
        })
    }

    fn refresh_access_token(
        &self,
        context: &ProviderRequestContext,
        auth: &mut CodexAuthState,
        now: chrono::DateTime<Utc>,
        warnings: &mut Vec<String>,
    ) -> Result<(), CodexError> {
        let refresh_token = auth
            .refresh_token
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or(CodexError::TokenExpired)?;
        let refreshed = self
            .client
            .refresh_token_with_context(context, refresh_token)?;
        if let Err(error) = auth.update_and_save_if_current(
            refreshed.access_token,
            refreshed.refresh_token,
            refreshed.id_token,
            now,
        ) {
            if matches!(error, CodexError::AccountChanged) {
                return Err(error);
            }
            crate::app_error!(
                "auth:codex",
                "failed to persist rotated credentials; using them for this session only"
            );
            warnings.push(
                "The refreshed Codex login is active for this session but could not be saved."
                    .into(),
            );
        }
        Ok(())
    }
}

fn account_identity_key(identity: &str) -> String {
    sha256_hex(identity.as_bytes())
}

fn validate_account_identity(
    expected: Option<&str>,
    observed: Option<&str>,
) -> Result<(), CodexError> {
    (expected == observed)
        .then_some(())
        .ok_or(CodexError::AccountChanged)
}

fn provider_error(error: CodexError) -> crate::providers::ProviderError {
    use crate::models::ProviderErrorKind as Kind;

    let kind = match error {
        CodexError::NotLoggedIn
        | CodexError::SessionExpired
        | CodexError::TokenConflict
        | CodexError::TokenRevoked
        | CodexError::TokenExpired
        | CodexError::InvalidAuth
        | CodexError::AccountChanged => Kind::Authentication,
        CodexError::ApiKeyOnly => Kind::Permission,
        CodexError::AuthWrite => Kind::CredentialStorage,
        CodexError::RequestFailed(429) => Kind::RateLimited,
        CodexError::RequestFailed(_) | CodexError::ConnectionFailed => Kind::Network,
        CodexError::InvalidResponse => Kind::InvalidResponse,
        CodexError::LocalUsage => Kind::LocalData,
        CodexError::Storage => Kind::Storage,
    };
    crate::providers::ProviderError::from_display(kind, error)
}

impl crate::providers::UsageProvider for CodexProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    fn accesses_system_keychain(&self) -> bool {
        true
    }

    fn has_local_credentials(&self) -> bool {
        CodexAuthState::has_local_credentials()
    }

    fn cache_identity(&self) -> crate::providers::CacheIdentity<'_> {
        self.account_identity
            .as_deref()
            .map(crate::providers::CacheIdentity::Resolved)
            .unwrap_or(crate::providers::CacheIdentity::Unresolved)
    }

    fn supports_account_names(&self) -> bool {
        true
    }

    fn account_identity(&self) -> Option<&str> {
        self.account_identity.as_deref()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, crate::providers::ProviderError> {
        CodexProvider::refresh(self).map_err(provider_error)
    }

    fn refresh_for_service(
        &self,
    ) -> Result<crate::providers::ProviderRefresh, crate::providers::ProviderError> {
        self.refresh_for_service_with_context(&ProviderRequestContext::direct(Arc::default()))
    }

    fn refresh_for_service_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<crate::providers::ProviderRefresh, crate::providers::ProviderError> {
        let (snapshot, identity) = self
            .refresh_with_identity(context)
            .map_err(provider_error)?;
        Ok(crate::providers::ProviderRefresh {
            snapshot,
            cache_identity: identity.clone(),
            account: identity.map(|identity| crate::providers::AccountRefresh {
                family: "codex",
                provider_id: "codex",
                identity,
            }),
        })
    }
}

#[cfg(test)]
mod account_tests {
    use std::{fs, sync::Arc, time::Duration};

    use tempfile::tempdir;

    use super::{
        account_identity_key, validate_account_identity, CodexClient, CodexError, CodexProvider,
    };
    use crate::{
        pricing::PricingStore,
        providers::{
            codex::auth::load_from_path_for_test, test_http, CacheIdentity, ProviderRequestContext,
            UsageProvider,
        },
        storage::Storage,
    };

    #[test]
    fn pinned_account_rejects_a_different_or_unreadable_login() {
        assert!(validate_account_identity(Some("account-a"), Some("account-a")).is_ok());
        assert!(matches!(
            validate_account_identity(Some("account-a"), Some("account-b")),
            Err(CodexError::AccountChanged)
        ));
        assert!(matches!(
            validate_account_identity(Some("account-a"), None),
            Err(CodexError::AccountChanged)
        ));
        assert!(matches!(
            validate_account_identity(None, Some("account-b")),
            Err(CodexError::AccountChanged)
        ));
        assert!(validate_account_identity(None, None).is_ok());
    }

    #[test]
    fn cache_identity_tracks_the_launch_resolved_account() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let pricing = Arc::new(PricingStore::new(directory.path().join("pricing")).unwrap());
        let provider = CodexProvider {
            account_identity: Some("account-a".into()),
            storage: storage.clone(),
            pricing: pricing.clone(),
            client: CodexClient::new().unwrap(),
        };
        let unresolved = CodexProvider {
            account_identity: None,
            storage,
            pricing,
            client: CodexClient::new().unwrap(),
        };

        assert_eq!(
            UsageProvider::cache_identity(&provider),
            CacheIdentity::Resolved("account-a")
        );
        assert_eq!(
            UsageProvider::cache_identity(&unresolved),
            CacheIdentity::Unresolved
        );
    }

    #[test]
    fn context_aware_refresh_routes_codex_requests_through_selected_proxy() {
        let directory = tempdir().unwrap();
        let auth_path = directory.path().join("auth.json");
        fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"secret-token","account_id":"account-a"}}"#,
        )
        .unwrap();
        let auth = load_from_path_for_test(&auth_path).unwrap();

        let (proxy_url, proxy_server) =
            test_http::serve_sequence_capturing_requests(&[(200, r#"{}"#), (200, r#"{}"#)]);
        let client = CodexClient::with_test_endpoints(
            &format!("{proxy_url}/usage"),
            &format!("{proxy_url}/reset-credits"),
            &format!("{proxy_url}/reset-credits/consume"),
            &format!("{proxy_url}/token"),
            Duration::from_secs(1),
        )
        .unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let pricing = Arc::new(PricingStore::new(directory.path().join("pricing")).unwrap());
        let account_identity = account_identity_key("account-a");
        let provider = CodexProvider {
            account_identity: Some(account_identity.clone()),
            storage,
            pricing,
            client,
        };
        let context = ProviderRequestContext {
            proxy_url: Some(reqwest::Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::default(),
        };

        let (response, reset_credits) = provider.fetch_remote_usage_data(&context, &auth).unwrap();

        assert!(response.status.is_success());
        assert!(reset_credits.is_some());
        let requests = proxy_server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].starts_with(&format!("GET {proxy_url}/usage HTTP/1.1")),
            "{}",
            requests[0]
        );
        assert!(
            requests[1].starts_with(&format!("GET {proxy_url}/reset-credits HTTP/1.1")),
            "{}",
            requests[1]
        );
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer secret-token"));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("chatgpt-account-id: account-a"));
    }
}
