#[cfg(not(target_os = "macos"))]
mod auth;
mod client;
mod discovery;
mod mapper;

use std::path::PathBuf;
#[cfg(not(target_os = "macos"))]
use std::sync::Arc;

use chrono::Utc;
#[cfg(not(target_os = "macos"))]
use serde_json::json;
use serde_json::Value;
use thiserror::Error;

use crate::providers::ProviderRequestContext;

use crate::models::{
    MetricDefinition, MetricSection, ProviderDefinition, ProviderSnapshot, UsageHistory,
};

use self::{client::AntigravityClient, discovery::discover};

use self::discovery::LanguageServer;
use self::mapper::{
    build_legacy_quotas, parse_command_model_configs, parse_plan, parse_quota_summary,
    parse_user_status,
};
#[cfg(not(target_os = "macos"))]
use self::{
    auth::{load_token, AccessTokenCache},
    client::{CloudOutcome, CloudUserAgent, RefreshOutcome},
    mapper::{parse_cloud_models, parse_quota_buckets},
};

#[cfg(not(target_os = "macos"))]
const QUOTA_SUMMARY_PATH: &str = "/v1internal:retrieveUserQuotaSummary";
#[cfg(not(target_os = "macos"))]
const FETCH_MODELS_PATH: &str = "/v1internal:fetchAvailableModels";
#[cfg(not(target_os = "macos"))]
const LOAD_CODE_ASSIST_PATH: &str = "/v1internal:loadCodeAssist";
#[cfg(not(target_os = "macos"))]
const RETRIEVE_QUOTA_PATH: &str = "/v1internal:retrieveUserQuota";

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: "antigravity".into(),
        display_name: "Antigravity".into(),
        short_name: "A".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![],
        metrics: vec![
            MetricDefinition::quota(
                "antigravity.geminiPro",
                "Session",
                "geminiPro",
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                "S",
            ),
            MetricDefinition::quota(
                "antigravity.geminiWeekly",
                "Weekly",
                "geminiWeekly",
                false,
                true,
                MetricSection::AlwaysVisible,
                true,
                "W",
            ),
            MetricDefinition::quota(
                "antigravity.claude",
                "Claude",
                "claude",
                true,
                true,
                MetricSection::OnDemand,
                false,
                "C",
            ),
            MetricDefinition::quota(
                "antigravity.claudeWeekly",
                "Claude Weekly",
                "claudeWeekly",
                false,
                true,
                MetricSection::OnDemand,
                false,
                "CW",
            ),
        ],
    }
}

#[derive(Debug, Error)]
pub enum AntigravityError {
    #[cfg(not(target_os = "macos"))]
    #[error("Start Antigravity or run `agy` and try again.")]
    NotSignedIn,
    #[cfg(not(target_os = "macos"))]
    #[error("Antigravity sign-in expired. Open Antigravity or run `agy` to refresh.")]
    AuthExpired,
    #[cfg(not(target_os = "macos"))]
    #[error("Antigravity credentials could not be read from secure storage.")]
    CredentialStoreUnreadable,
    #[cfg(not(target_os = "macos"))]
    #[error("Antigravity credentials are invalid. Sign in again in Antigravity or `agy`.")]
    InvalidCredentialData,
    #[error("Antigravity usage is temporarily unavailable. Try again shortly.")]
    Unavailable,
}

pub struct AntigravityProvider {
    client: AntigravityClient,
    #[cfg(not(target_os = "macos"))]
    access_token_cache: AccessTokenCache,
}

impl AntigravityProvider {
    pub fn new(_cache_path: PathBuf) -> Result<Self, AntigravityError> {
        let client = AntigravityClient::new()?;
        Ok(Self {
            client,
            #[cfg(not(target_os = "macos"))]
            access_token_cache: AccessTokenCache::new(_cache_path),
        })
    }

    #[cfg(target_os = "macos")]
    fn refresh_inner(&self) -> Result<ProviderSnapshot, AntigravityError> {
        self.refresh_inner_with(discover)
    }

    #[cfg(target_os = "macos")]
    fn refresh_inner_with(
        &self,
        discover_server: impl FnOnce() -> Option<LanguageServer>,
    ) -> Result<ProviderSnapshot, AntigravityError> {
        refresh_local_with(discover_server, |server, method| {
            self.client.call_language_server(server, method)
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn refresh_inner(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<ProviderSnapshot, AntigravityError> {
        if let Ok(snapshot) = refresh_local_with(discover, |server, method| {
            self.client.call_language_server(server, method)
        }) {
            return Ok(snapshot);
        }

        let keychain = match load_token()? {
            Some(token) => token,
            None => {
                self.access_token_cache.discard();
                return Err(AntigravityError::NotSignedIn);
            }
        };
        let now = Utc::now();
        let cached = self
            .access_token_cache
            .load(keychain.refresh_token.as_deref(), now);
        let (access_tokens, access_is_expired) = access_token_candidates(&keychain, cached, now);

        let mut saw_auth_failure = access_is_expired;
        let mut saw_unavailable = false;
        for access_token in &access_tokens {
            match self.fetch_remote(context, access_token) {
                Ok(snapshot) => return Ok(snapshot),
                Err(AntigravityError::AuthExpired) => saw_auth_failure = true,
                Err(AntigravityError::Unavailable) => saw_unavailable = true,
                Err(error) => return Err(error),
            }
        }

        let refresh_token = keychain
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty());
        if let Some(refresh_token) = refresh_token.filter(|_| {
            should_refresh_access_token(saw_auth_failure, !access_tokens.is_empty(), true)
        }) {
            match self
                .client
                .refresh_google_token_with_context(context, refresh_token)
            {
                RefreshOutcome::Refreshed {
                    access_token,
                    expires_in_seconds,
                } => {
                    crate::app_info!("auth:antigravity", "token refresh succeeded");
                    self.access_token_cache.store(
                        &access_token,
                        expires_in_seconds,
                        Some(refresh_token),
                        Utc::now(),
                    );
                    return match self.fetch_remote(context, &access_token) {
                        Err(AntigravityError::AuthExpired) => {
                            self.access_token_cache.discard();
                            Err(AntigravityError::AuthExpired)
                        }
                        result => result,
                    };
                }
                RefreshOutcome::AuthFailed => {
                    self.access_token_cache.discard();
                    return Err(AntigravityError::AuthExpired);
                }
                RefreshOutcome::Unavailable => return Err(AntigravityError::Unavailable),
            }
        }
        Err(credential_failure(
            saw_auth_failure,
            saw_unavailable,
            !access_tokens.is_empty(),
        ))
    }

    #[cfg(not(target_os = "macos"))]
    fn fetch_remote(
        &self,
        context: &ProviderRequestContext,
        token: &str,
    ) -> Result<ProviderSnapshot, AntigravityError> {
        match self.client.cloud_code_with_context(
            context,
            QUOTA_SUMMARY_PATH,
            token,
            json!({}),
            CloudUserAgent::Antigravity,
        ) {
            CloudOutcome::Ok(value) => {
                if let Some(quotas) = parse_quota_summary(&value) {
                    return Ok(snapshot(self.load_remote_plan(context, token), quotas));
                }
            }
            CloudOutcome::AuthFailed => return Err(AntigravityError::AuthExpired),
            CloudOutcome::Unavailable => {}
        }

        match self.client.cloud_code_with_context(
            context,
            FETCH_MODELS_PATH,
            token,
            json!({}),
            CloudUserAgent::Antigravity,
        ) {
            CloudOutcome::Ok(value) => {
                let quotas = build_legacy_quotas(parse_cloud_models(&value));
                if !quotas.is_empty() {
                    return Ok(snapshot(self.load_remote_plan(context, token), quotas));
                }
            }
            CloudOutcome::AuthFailed => return Err(AntigravityError::AuthExpired),
            CloudOutcome::Unavailable => {}
        }

        let (plan, project) = match self.client.cloud_code_with_context(
            context,
            LOAD_CODE_ASSIST_PATH,
            token,
            json!({}),
            CloudUserAgent::Agy,
        ) {
            CloudOutcome::Ok(value) => (
                remote_plan(&value),
                value
                    .get("cloudaicompanionProject")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_owned),
            ),
            CloudOutcome::AuthFailed => return Err(AntigravityError::AuthExpired),
            CloudOutcome::Unavailable => (None, None),
        };

        let body = project
            .as_ref()
            .map(|project| json!({"project": project}))
            .unwrap_or_else(|| json!({}));
        let mut quota = self.client.cloud_code_with_context(
            context,
            RETRIEVE_QUOTA_PATH,
            token,
            body,
            CloudUserAgent::Agy,
        );
        if matches!(quota, CloudOutcome::Unavailable) && project.is_some() {
            quota = self.client.cloud_code_with_context(
                context,
                RETRIEVE_QUOTA_PATH,
                token,
                json!({}),
                CloudUserAgent::Agy,
            );
        }
        match quota {
            CloudOutcome::Ok(value) => {
                let quotas = build_legacy_quotas(parse_quota_buckets(&value));
                if !quotas.is_empty() {
                    return Ok(snapshot(plan, quotas));
                }
            }
            CloudOutcome::AuthFailed => return Err(AntigravityError::AuthExpired),
            CloudOutcome::Unavailable => {}
        }
        Err(AntigravityError::Unavailable)
    }

    #[cfg(not(target_os = "macos"))]
    fn load_remote_plan(&self, context: &ProviderRequestContext, token: &str) -> Option<String> {
        match self.client.cloud_code_with_context(
            context,
            LOAD_CODE_ASSIST_PATH,
            token,
            json!({}),
            CloudUserAgent::Agy,
        ) {
            CloudOutcome::Ok(value) => remote_plan(&value),
            _ => None,
        }
    }
}

fn refresh_local_with(
    discover_server: impl FnOnce() -> Option<LanguageServer>,
    mut call: impl FnMut(&LanguageServer, &str) -> Option<Value>,
) -> Result<ProviderSnapshot, AntigravityError> {
    let server = discover_server().ok_or(AntigravityError::Unavailable)?;

    if let Some(summary) = call(&server, "RetrieveUserQuotaSummary") {
        if let Some(quotas) = parse_quota_summary(&summary).filter(|quotas| !quotas.is_empty()) {
            let plan = call(&server, "GetUserStatus").as_ref().and_then(parse_plan);
            return Ok(snapshot(plan, quotas));
        }
    }

    if let Some(status) = call(&server, "GetUserStatus") {
        if let Some((plan, configs)) = parse_user_status(&status) {
            let quotas = build_legacy_quotas(configs);
            if !quotas.is_empty() {
                return Ok(snapshot(plan, quotas));
            }
        }
        if let Some(configs) = call(&server, "GetCommandModelConfigs")
            .as_ref()
            .and_then(parse_command_model_configs)
        {
            let quotas = build_legacy_quotas(configs);
            if !quotas.is_empty() {
                return Ok(snapshot(None, quotas));
            }
        }
    }

    Err(AntigravityError::Unavailable)
}

#[cfg(not(target_os = "macos"))]
fn access_token_candidates(
    source: &auth::AntigravityToken,
    cached: Option<String>,
    now: chrono::DateTime<Utc>,
) -> (Vec<String>, bool) {
    let access_is_expired = source.access_token.as_ref().is_some_and(|_| {
        source
            .expiry
            .is_some_and(|expiry| expiry <= now + chrono::Duration::seconds(60))
    });
    let mut candidates = Vec::new();
    if !access_is_expired {
        if let Some(access_token) = source
            .access_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            candidates.push(access_token.to_owned());
        }
    }
    if let Some(cached) = cached.filter(|token| !token.trim().is_empty()) {
        if !candidates.iter().any(|token| token == &cached) {
            candidates.push(cached);
        }
    }
    (candidates, access_is_expired)
}

#[cfg(not(target_os = "macos"))]
fn should_refresh_access_token(
    saw_auth_failure: bool,
    has_access_candidate: bool,
    has_refresh_token: bool,
) -> bool {
    has_refresh_token && (saw_auth_failure || !has_access_candidate)
}

#[cfg(not(target_os = "macos"))]
fn credential_failure(
    saw_auth_failure: bool,
    saw_unavailable: bool,
    tried_access_token: bool,
) -> AntigravityError {
    if saw_auth_failure {
        AntigravityError::AuthExpired
    } else if saw_unavailable || tried_access_token {
        AntigravityError::Unavailable
    } else {
        AntigravityError::NotSignedIn
    }
}

#[cfg(not(target_os = "macos"))]
fn remote_plan(value: &Value) -> Option<String> {
    let raw = value
        .pointer("/paidTier/name")
        .or_else(|| value.pointer("/currentTier/name"))
        .and_then(Value::as_str)?;
    for tier in ["Ultra", "Pro", "Free"] {
        if raw
            .to_ascii_lowercase()
            .contains(&tier.to_ascii_lowercase())
        {
            return Some(tier.into());
        }
    }
    Some(raw.into())
}

fn snapshot(plan: Option<String>, quotas: Vec<crate::models::QuotaWindow>) -> ProviderSnapshot {
    ProviderSnapshot {
        credit_packages: Vec::new(),
        provider_id: "antigravity".into(),
        plan,
        quotas,
        value_metrics: Vec::new(),
        status_metrics: Vec::new(),
        notices: Vec::new(),
        usage: UsageHistory::default(),
        warnings: Vec::new(),
        refreshed_at: Utc::now(),
    }
}

impl crate::providers::UsageProvider for AntigravityProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    fn has_local_credentials(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            discover().is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            auth::has_local_credentials() || discover().is_some()
        }
    }

    fn refresh(&self) -> Result<ProviderSnapshot, crate::providers::ProviderError> {
        #[cfg(target_os = "macos")]
        {
            self.refresh_inner().map_err(provider_error)
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.refresh_with_context(&ProviderRequestContext::direct(Arc::default()))
        }
    }

    fn refresh_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<ProviderSnapshot, crate::providers::ProviderError> {
        #[cfg(target_os = "macos")]
        {
            let _ = context;
            self.refresh_inner().map_err(provider_error)
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.refresh_inner(context).map_err(provider_error)
        }
    }
}

fn provider_error(error: AntigravityError) -> crate::providers::ProviderError {
    use crate::models::ProviderErrorKind as Kind;
    let kind = match error {
        #[cfg(not(target_os = "macos"))]
        AntigravityError::NotSignedIn | AntigravityError::AuthExpired => Kind::Authentication,
        #[cfg(not(target_os = "macos"))]
        AntigravityError::InvalidCredentialData => Kind::InvalidResponse,
        #[cfg(not(target_os = "macos"))]
        AntigravityError::CredentialStoreUnreadable => Kind::CredentialStorage,
        AntigravityError::Unavailable => {
            #[cfg(target_os = "macos")]
            {
                Kind::LocalServiceUnavailable
            }
            #[cfg(not(target_os = "macos"))]
            {
                Kind::Network
            }
        }
    };
    crate::providers::ProviderError::from_display(kind, error)
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "macos"))]
    use chrono::TimeZone;
    use serde_json::{json, Value};
    use std::cell::Cell;

    #[cfg(not(target_os = "macos"))]
    use super::auth::AntigravityToken;
    use super::refresh_local_with;
    #[cfg(not(target_os = "macos"))]
    use super::{access_token_candidates, credential_failure, should_refresh_access_token};
    use super::{provider_error, AntigravityError};
    use crate::providers::antigravity::discovery::LanguageServer;

    #[cfg(target_os = "macos")]
    #[test]
    fn unavailable_local_service_has_a_distinct_provider_category() {
        assert_eq!(
            provider_error(AntigravityError::Unavailable).kind(),
            crate::models::ProviderErrorKind::LocalServiceUnavailable
        );
    }

    fn server() -> LanguageServer {
        LanguageServer {
            csrf: "csrf".into(),
            ports: vec![1234],
            extension_port: None,
        }
    }

    #[test]
    fn local_refresh_accepts_authoritative_quota_summary() {
        let response = json!({"response":{"groups":[{"buckets":[
            {"bucketId":"gemini-5h","remainingFraction":0.8}
        ]}]}});
        let snapshot = refresh_local_with(
            || Some(server()),
            |_, method| match method {
                "RetrieveUserQuotaSummary" => Some(response.clone()),
                "GetUserStatus" => {
                    Some(json!({"userStatus":{"userTier":{"name":"Google AI Pro"}}}))
                }
                _ => None,
            },
        )
        .unwrap();

        assert_eq!(snapshot.quotas.len(), 1);
        assert_eq!(snapshot.quotas[0].used_percent, 20.0);
        assert_eq!(snapshot.plan.as_deref(), Some("Pro"));
    }

    #[test]
    fn local_refresh_accepts_user_status_quota_shape() {
        let calls = Cell::new(0);
        let snapshot = refresh_local_with(
            || Some(server()),
            |_, method| {
                calls.set(calls.get() + 1);
                (method == "RetrieveUserQuotaSummary")
                    .then(|| json!({"groups":[]}))
                    .or_else(|| {
                        (method == "GetUserStatus").then(|| {
                            json!({"userStatus":{
                                "userTier":{"name":"Google AI Pro"},
                                "cascadeModelConfigData":{"clientModelConfigs":[
                                    {"label":"Gemini Pro","quotaInfo":{"remainingFraction":0.6}}
                                ]}
                            }})
                        })
                    })
            },
        )
        .unwrap();

        assert_eq!(calls.get(), 2);
        assert_eq!(snapshot.quotas[0].used_percent, 40.0);
        assert_eq!(snapshot.plan.as_deref(), Some("Pro"));
    }

    #[test]
    fn local_refresh_accepts_command_model_config_shape() {
        let snapshot = refresh_local_with(
            || Some(server()),
            |_, method| match method {
                "RetrieveUserQuotaSummary" => Some(json!({"groups":[]})),
                "GetUserStatus" => Some(json!({"userStatus":{}})),
                "GetCommandModelConfigs" => Some(json!({"clientModelConfigs":[
                    {"label":"Claude Sonnet","quotaInfo":{"remainingFraction":0.25}}
                ]})),
                _ => None,
            },
        )
        .unwrap();

        assert_eq!(snapshot.quotas.len(), 1);
        assert_eq!(snapshot.quotas[0].id, "claude");
        assert_eq!(snapshot.quotas[0].used_percent, 75.0);
    }

    #[test]
    fn no_server_returns_unavailable_without_remote_fallback() {
        let remote_calls = Cell::new(0);
        let result = refresh_local_with(
            || None,
            |_, _| {
                remote_calls.set(remote_calls.get() + 1);
                None
            },
        );

        assert!(matches!(result, Err(AntigravityError::Unavailable)));
        assert_eq!(remote_calls.get(), 0);
    }

    #[test]
    fn failed_rpc_returns_unavailable_without_remote_fallback() {
        let local_rpc_calls = Cell::new(0);
        let result = refresh_local_with(
            || Some(server()),
            |_, _| {
                local_rpc_calls.set(local_rpc_calls.get() + 1);
                None
            },
        );

        assert!(matches!(result, Err(AntigravityError::Unavailable)));
        assert_eq!(local_rpc_calls.get(), 2);
    }

    #[test]
    fn malformed_local_quota_results_return_unavailable_without_fallback() {
        let malformed: Value =
            json!({"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":"bad"}]}]});
        let result = refresh_local_with(
            || Some(server()),
            |_, method| {
                (method == "RetrieveUserQuotaSummary")
                    .then(|| malformed.clone())
                    .or_else(|| (method == "GetUserStatus").then(|| json!({"userStatus":{}})))
                    .or_else(|| {
                        (method == "GetCommandModelConfigs")
                            .then(|| json!({"clientModelConfigs":[]}))
                    })
            },
        );

        assert!(matches!(result, Err(AntigravityError::Unavailable)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_refresh_entrypoint_uses_rpc_without_cloud_or_token_calls() {
        let local_rpc = crate::providers::test_http::serve_sequence(&[
            (
                200,
                r#"{"response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.8}]}]}}"#,
            ),
            (
                200,
                r#"{"userStatus":{"userTier":{"name":"Google AI Pro"}}}"#,
            ),
        ]);
        let port = local_rpc
            .strip_prefix("http://127.0.0.1:")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let client = super::client::AntigravityClient::with_endpoints(
            vec!["http://cloud.invalid".into()],
            "http://tokens.invalid/token".into(),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let provider = super::AntigravityProvider { client };

        let snapshot = provider
            .refresh_inner_with(|| {
                Some(LanguageServer {
                    csrf: "csrf".into(),
                    ports: vec![],
                    extension_port: Some(port),
                })
            })
            .unwrap();

        assert_eq!(snapshot.quotas.len(), 1);
        assert_eq!(snapshot.quotas[0].used_percent, 20.0);
        assert_eq!(provider.client.fallback_call_counts(), (0, 0));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_auth_refresh_helpers_keep_the_existing_remote_flow() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).unwrap();
        let source = AntigravityToken {
            access_token: Some("credential-access".into()),
            refresh_token: Some("refresh".into()),
            expiry: Some(now + chrono::Duration::hours(1)),
        };
        let (candidates, expired) = access_token_candidates(&source, Some("cached".into()), now);
        assert_eq!(candidates, ["credential-access", "cached"]);
        assert!(!expired);
        assert!(should_refresh_access_token(true, true, true));
        assert!(matches!(
            credential_failure(false, true, true),
            AntigravityError::Unavailable
        ));
    }
}
