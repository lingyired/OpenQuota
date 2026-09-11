mod auth;
mod client;
mod mapper;
mod usage;

use std::sync::Arc;

use chrono::{DateTime, Days, Local, TimeZone, Utc};
use serde_json::Value;
use thiserror::Error;

use crate::models::{
    CreditPackage, MetricDefinition, MetricSection, MetricSource, MetricValue, MetricValueKind,
    ProviderDefinition, ProviderErrorKind, ProviderLink, ProviderNotice, ProviderNoticeTone,
    ProviderSnapshot, QuotaWindow, StatusMetric, UsageCompleteness, UsageHistory,
    UsagePeriodSelection, ValueMetric,
};

use self::{
    auth::{WorkBuddyAuth, WorkBuddyAuthError},
    client::{EndpointResponse, WorkBuddyClient, WorkBuddyClientError},
    mapper::{map_resources, MappedResources},
    usage::{build_history, collect_pages, parse_page, ParsedUsagePage, MAX_PAGES},
};
use super::{ProviderError, ProviderRefresh, UsageProvider};

const PROVIDER_ID: &str = "workbuddy";
const SOURCE_NOTE: &str = "WorkBuddy official usage";

pub(crate) fn definition() -> ProviderDefinition {
    ProviderDefinition {
        id: PROVIDER_ID.into(),
        display_name: "Workbuddy CN".into(),
        short_name: "WB".into(),
        fallback_enabled: false,
        local_usage_source_note: None,
        links: vec![
            ProviderLink::new("Dashboard", "https://www.workbuddy.cn/profile/plans-usage"),
            ProviderLink::new("WorkBuddy", "https://www.workbuddy.cn/"),
        ],
        metrics: vec![
            MetricDefinition::value(
                "workbuddy.credits",
                "Credits",
                "balance",
                true,
                MetricSection::AlwaysVisible,
                true,
                "C",
                None,
            ),
            MetricDefinition::value(
                "workbuddy.nearestExpiring",
                "近期到期的积分包",
                "nearestExpiring",
                true,
                MetricSection::AlwaysVisible,
                false,
                "近",
                None,
            ),
            MetricDefinition::new(
                "workbuddy.creditPackages",
                "可用积分包",
                MetricSource::CreditPackages,
                false,
                true,
                MetricSection::AlwaysVisible,
                false,
                None,
                None,
            ),
            MetricDefinition::usage(
                "workbuddy.today",
                "Today",
                UsagePeriodSelection::Today,
                MetricSection::OnDemand,
                "T",
            ),
            MetricDefinition::usage(
                "workbuddy.yesterday",
                "Yesterday",
                UsagePeriodSelection::Yesterday,
                MetricSection::OnDemand,
                "Y",
            ),
            MetricDefinition::usage(
                "workbuddy.last30",
                "Last 30 Days",
                UsagePeriodSelection::Last30Days,
                MetricSection::OnDemand,
                "M",
            ),
            MetricDefinition::trend("workbuddy.trend"),
        ],
    }
}

#[derive(Debug, Error)]
pub(crate) enum WorkBuddyError {
    #[error("WorkBuddy is not logged in. Sign in to WorkBuddy or CodeBuddy first.")]
    NotLoggedIn,
    #[error("WorkBuddy login data is invalid. Sign in again to WorkBuddy or CodeBuddy.")]
    InvalidAuth,
    #[error("WorkBuddy credentials could not be read or updated.")]
    CredentialStorage,
    #[error("WorkBuddy access token expired and could not be refreshed. Sign in again.")]
    TokenExpired,
    #[error("WorkBuddy token refresh failed. Sign in again.")]
    RefreshFailed,
    #[error("Could not reach WorkBuddy. Check your internet connection.")]
    Connection,
    #[error("WorkBuddy returned an invalid response.")]
    InvalidResponse,
    #[error("WorkBuddy request was blocked by the upstream WAF (10085). Try again later.")]
    WafBlocked,
    #[error("WorkBuddy request failed (HTTP {0}).")]
    RequestFailed(u16),
    #[error("WorkBuddy returned no usable package or usage data.")]
    NoData,
}

impl From<WorkBuddyAuthError> for WorkBuddyError {
    fn from(error: WorkBuddyAuthError) -> Self {
        match error {
            WorkBuddyAuthError::NotLoggedIn => Self::NotLoggedIn,
            WorkBuddyAuthError::Invalid => Self::InvalidAuth,
            WorkBuddyAuthError::Storage => Self::CredentialStorage,
        }
    }
}

impl From<WorkBuddyClientError> for WorkBuddyError {
    fn from(error: WorkBuddyClientError) -> Self {
        match error {
            WorkBuddyClientError::Connection => Self::Connection,
            WorkBuddyClientError::InvalidResponse => Self::InvalidResponse,
            WorkBuddyClientError::RefreshFailed => Self::RefreshFailed,
        }
    }
}

impl From<WorkBuddyError> for ProviderError {
    fn from(error: WorkBuddyError) -> Self {
        let kind = match error {
            WorkBuddyError::NotLoggedIn
            | WorkBuddyError::InvalidAuth
            | WorkBuddyError::TokenExpired
            | WorkBuddyError::RefreshFailed
            | WorkBuddyError::RequestFailed(401) => ProviderErrorKind::Authentication,
            WorkBuddyError::RequestFailed(403) => ProviderErrorKind::Permission,
            WorkBuddyError::CredentialStorage => ProviderErrorKind::CredentialStorage,
            WorkBuddyError::RequestFailed(429) => ProviderErrorKind::RateLimited,
            WorkBuddyError::Connection => ProviderErrorKind::Network,
            WorkBuddyError::InvalidResponse | WorkBuddyError::NoData => {
                ProviderErrorKind::InvalidResponse
            }
            WorkBuddyError::WafBlocked | WorkBuddyError::RequestFailed(_) => {
                ProviderErrorKind::Network
            }
        };
        ProviderError::from_display(kind, error)
    }
}

pub struct WorkBuddyProvider {
    client: Arc<WorkBuddyClient>,
    account_identity: Option<String>,
}

impl WorkBuddyProvider {
    pub fn new() -> Result<Self, WorkBuddyError> {
        let account_identity = WorkBuddyAuth::load()
            .ok()
            .and_then(|auth| auth.uid)
            .map(|uid| crate::hashing::sha256_hex(uid.as_bytes()));
        Ok(Self {
            client: Arc::new(WorkBuddyClient::new()?),
            account_identity,
        })
    }

    fn refresh_with_identity(&self) -> Result<(ProviderSnapshot, Option<String>), WorkBuddyError> {
        let now = Local::now();
        let mut auth = WorkBuddyAuth::load()?;
        let account_identity = auth
            .uid
            .as_deref()
            .map(|uid| crate::hashing::sha256_hex(uid.as_bytes()));
        let mut warnings = Vec::new();
        let mut refresh_attempted = false;

        let mut resources = self.fetch_resources(&auth, now);
        if resources.iter().any(ResourceOutcome::is_unauthorized) {
            if auth
                .refresh_token
                .as_deref()
                .is_some_and(|token| !token.trim().is_empty())
            {
                refresh_attempted = true;
                match self.refresh_auth(&mut auth, &mut warnings) {
                    Ok(()) => {
                        resources = self.retry_unauthorized_resources(&auth, now, resources);
                    }
                    Err(error) => warnings.push(error.to_string()),
                }
            } else {
                warnings
                    .push("WorkBuddy access token expired; no refresh token is available.".into());
            }
        }
        append_resource_warnings(&resources, &mut warnings);

        let resource_bodies = resources.map_successes();
        let mapped_resources = match map_resources(
            resource_bodies[0],
            resource_bodies[1],
            resource_bodies[2],
            now,
        ) {
            Ok(mapped) => Some(mapped),
            Err(error) => {
                if resource_bodies.iter().any(Option::is_some) {
                    warnings.push(error.to_string());
                }
                None
            }
        };

        let start = now
            .date_naive()
            .checked_sub_days(Days::new(30))
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .and_then(|value| Local.from_local_datetime(&value).single())
            .unwrap_or(now);
        let usage_attempt = self.fetch_usage_pages(&auth, start, now, Vec::new(), 1);
        let (usage, usage_succeeded) = self.finish_usage(
            &mut auth,
            start,
            now,
            usage_attempt,
            &mut refresh_attempted,
            &mut warnings,
        );

        if mapped_resources.is_none() && !usage_succeeded {
            return Err(select_failure(&resources, usage_succeeded));
        }

        let snapshot = build_snapshot(mapped_resources.as_ref(), usage, warnings, now);
        Ok((snapshot, account_identity))
    }

    fn fetch_resources(&self, auth: &WorkBuddyAuth, now: DateTime<Local>) -> ResourceOutcomes {
        let (summary, paid, free) = std::thread::scope(|scope| {
            let summary = scope.spawn(|| self.client.fetch_resource_summary(auth, now));
            let paid = scope.spawn(|| self.client.fetch_paid_packages(auth, now));
            let free = scope.spawn(|| self.client.fetch_free_packages(auth, now));
            (
                summary
                    .join()
                    .unwrap_or(Err(WorkBuddyClientError::Connection)),
                paid.join().unwrap_or(Err(WorkBuddyClientError::Connection)),
                free.join().unwrap_or(Err(WorkBuddyClientError::Connection)),
            )
        });
        ResourceOutcomes([
            classify_endpoint(summary),
            classify_endpoint(paid),
            classify_endpoint(free),
        ])
    }

    fn retry_unauthorized_resources(
        &self,
        auth: &WorkBuddyAuth,
        now: DateTime<Local>,
        resources: ResourceOutcomes,
    ) -> ResourceOutcomes {
        let [summary, paid, free] = resources.0;
        ResourceOutcomes([
            retry_resource(summary, || self.client.fetch_resource_summary(auth, now)),
            retry_resource(paid, || self.client.fetch_paid_packages(auth, now)),
            retry_resource(free, || self.client.fetch_free_packages(auth, now)),
        ])
    }

    fn refresh_auth(
        &self,
        auth: &mut WorkBuddyAuth,
        warnings: &mut Vec<String>,
    ) -> Result<(), WorkBuddyError> {
        if auth
            .refresh_token
            .as_deref()
            .is_none_or(|token| token.trim().is_empty())
        {
            return Err(WorkBuddyError::TokenExpired);
        }
        let (access_token, refresh_token) = self.client.refresh_token(auth)?;
        if let Err(error) = auth.save_tokens(access_token.clone(), refresh_token.clone()) {
            warnings.push(format!(
                "WorkBuddy token was refreshed for this session but could not be saved: {error}"
            ));
            auth.access_token = access_token;
            if refresh_token.is_some() {
                auth.refresh_token = refresh_token;
            }
        }
        Ok(())
    }

    fn fetch_usage_pages(
        &self,
        auth: &WorkBuddyAuth,
        start: DateTime<Local>,
        end: DateTime<Local>,
        mut pages: Vec<ParsedUsagePage>,
        mut page_number: u32,
    ) -> UsageFetchOutcome {
        loop {
            let response = match self.client.fetch_usage_page(auth, start, end, page_number) {
                Ok(response) => response,
                Err(error) => {
                    let total = latest_total(&pages);
                    return UsageFetchOutcome::Failed {
                        pages,
                        total,
                        error: error.into(),
                    };
                }
            };
            if response.is_waf() {
                let total = latest_total(&pages);
                return UsageFetchOutcome::Failed {
                    pages,
                    total,
                    error: WorkBuddyError::WafBlocked,
                };
            }
            if response.is_http_forbidden() {
                let total = latest_total(&pages);
                return UsageFetchOutcome::Failed {
                    pages,
                    total,
                    error: WorkBuddyError::RequestFailed(response.status.as_u16()),
                };
            }
            if response.is_unauthorized() {
                let total = latest_total(&pages);
                return UsageFetchOutcome::Unauthorized {
                    pages,
                    page_number,
                    total,
                };
            }
            if !response.is_success() {
                let total = latest_total(&pages);
                return UsageFetchOutcome::Failed {
                    pages,
                    total,
                    error: WorkBuddyError::RequestFailed(response.status.as_u16()),
                };
            }
            let Some(parsed) = parse_page(&response.body, start, end) else {
                let total = latest_total(&pages);
                return UsageFetchOutcome::Failed {
                    pages,
                    total,
                    error: WorkBuddyError::InvalidResponse,
                };
            };
            let total = parsed.total;
            let raw_len = parsed.raw_len;
            pages.push(parsed);
            let fetched_raw = pages.iter().map(|page| page.raw_len).sum::<usize>();
            if fetched_raw >= total || raw_len == 0 {
                return UsageFetchOutcome::Finished {
                    pages,
                    complete: fetched_raw >= total,
                    total,
                };
            }
            if page_number >= MAX_PAGES {
                return UsageFetchOutcome::Finished {
                    pages,
                    complete: false,
                    total,
                };
            }
            page_number += 1;
        }
    }

    fn finish_usage(
        &self,
        auth: &mut WorkBuddyAuth,
        start: DateTime<Local>,
        end: DateTime<Local>,
        attempt: UsageFetchOutcome,
        refresh_attempted: &mut bool,
        warnings: &mut Vec<String>,
    ) -> (UsageHistory, bool) {
        let attempt = if let UsageFetchOutcome::Unauthorized {
            pages, page_number, ..
        } = attempt
        {
            if !*refresh_attempted
                && auth
                    .refresh_token
                    .as_deref()
                    .is_some_and(|token| !token.trim().is_empty())
            {
                *refresh_attempted = true;
                match self.refresh_auth(auth, warnings) {
                    Ok(()) => self.fetch_usage_pages(auth, start, end, pages, page_number),
                    Err(error) => {
                        warnings.push(error.to_string());
                        UsageFetchOutcome::Unauthorized {
                            pages,
                            page_number,
                            total: 0,
                        }
                    }
                }
            } else {
                warnings.push(
                    "WorkBuddy usage request was unauthorized and could not be retried.".into(),
                );
                UsageFetchOutcome::Unauthorized {
                    pages,
                    page_number,
                    total: 0,
                }
            }
        } else {
            attempt
        };

        let failure_message = match &attempt {
            UsageFetchOutcome::Failed { error, .. } => Some(error.to_string()),
            _ => None,
        };
        match attempt {
            UsageFetchOutcome::Finished {
                pages,
                complete,
                total,
            } => {
                let collection = collect_pages(&pages, complete, total);
                let history = build_history(&collection, end, SOURCE_NOTE);
                append_usage_warning(history.completeness, warnings);
                (history, true)
            }
            UsageFetchOutcome::Unauthorized { pages, total, .. }
            | UsageFetchOutcome::Failed { pages, total, .. } => {
                let collection = collect_pages(&pages, false, total);
                if collection.completeness == UsageCompleteness::Unavailable {
                    if let Some(message) = failure_message {
                        warnings.push(message);
                    }
                    append_usage_warning(UsageCompleteness::Unavailable, warnings);
                    (UsageHistory::default(), false)
                } else {
                    let history = build_history(&collection, end, SOURCE_NOTE);
                    append_usage_warning(history.completeness, warnings);
                    (history, true)
                }
            }
        }
    }

    pub fn refresh(&self) -> Result<ProviderSnapshot, WorkBuddyError> {
        self.refresh_with_identity().map(|(snapshot, _)| snapshot)
    }
}

impl UsageProvider for WorkBuddyProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    fn has_local_credentials(&self) -> bool {
        WorkBuddyAuth::has_local_credentials()
    }

    fn cache_identity(&self) -> super::CacheIdentity<'_> {
        self.account_identity
            .as_deref()
            .map(super::CacheIdentity::Resolved)
            .unwrap_or(super::CacheIdentity::Unresolved)
    }

    fn supports_account_names(&self) -> bool {
        true
    }

    fn account_identity(&self) -> Option<&str> {
        self.account_identity.as_deref()
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        WorkBuddyProvider::refresh(self).map_err(ProviderError::from)
    }

    fn refresh_for_service(&self) -> Result<ProviderRefresh, ProviderError> {
        let (snapshot, identity) = self.refresh_with_identity().map_err(ProviderError::from)?;
        Ok(ProviderRefresh {
            snapshot,
            cache_identity: identity.clone(),
            account: identity.map(|identity| super::AccountRefresh {
                family: PROVIDER_ID,
                provider_id: PROVIDER_ID,
                identity,
            }),
        })
    }
}

struct ResourceOutcomes([ResourceOutcome; 3]);

impl ResourceOutcomes {
    fn iter(&self) -> impl Iterator<Item = &ResourceOutcome> {
        self.0.iter()
    }

    fn map_successes(&self) -> [Option<&Value>; 3] {
        self.0.each_ref().map(|outcome| match outcome {
            ResourceOutcome::Success(body) => Some(body),
            ResourceOutcome::Unauthorized | ResourceOutcome::Failed(_) => None,
        })
    }
}

#[derive(Debug)]
enum ResourceOutcome {
    Success(Value),
    Unauthorized,
    Failed(WorkBuddyError),
}

impl ResourceOutcome {
    fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized)
    }
}

fn classify_endpoint(result: Result<EndpointResponse, WorkBuddyClientError>) -> ResourceOutcome {
    match result {
        Ok(response) if response.is_waf() => ResourceOutcome::Failed(WorkBuddyError::WafBlocked),
        Ok(response) if response.is_http_forbidden() => {
            ResourceOutcome::Failed(WorkBuddyError::RequestFailed(response.status.as_u16()))
        }
        Ok(response) if response.is_unauthorized() => ResourceOutcome::Unauthorized,
        Ok(response) if response.is_success() => ResourceOutcome::Success(response.body),
        Ok(response) => {
            ResourceOutcome::Failed(WorkBuddyError::RequestFailed(response.status.as_u16()))
        }
        Err(error) => ResourceOutcome::Failed(error.into()),
    }
}

fn retry_resource(
    outcome: ResourceOutcome,
    request: impl FnOnce() -> Result<EndpointResponse, WorkBuddyClientError>,
) -> ResourceOutcome {
    if outcome.is_unauthorized() {
        classify_endpoint(request())
    } else {
        outcome
    }
}

fn append_resource_warnings(resources: &ResourceOutcomes, warnings: &mut Vec<String>) {
    let labels = ["summary", "paid package", "free package"];
    for (label, outcome) in labels.into_iter().zip(resources.iter()) {
        match outcome {
            ResourceOutcome::Success(_) => {}
            ResourceOutcome::Unauthorized => {
                warnings.push(format!("WorkBuddy {label} data was unauthorized."));
            }
            ResourceOutcome::Failed(error) => {
                warnings.push(format!("WorkBuddy {label} data is unavailable: {error}"))
            }
        }
    }
}

fn append_usage_warning(completeness: UsageCompleteness, warnings: &mut Vec<String>) {
    match completeness {
        UsageCompleteness::Complete => {}
        UsageCompleteness::Partial => warnings
            .push("WorkBuddy usage data is partial; some records could not be loaded.".into()),
        UsageCompleteness::Unavailable => {
            warnings.push("WorkBuddy usage data is unavailable.".into())
        }
    }
}

fn select_failure(resources: &ResourceOutcomes, usage_succeeded: bool) -> WorkBuddyError {
    resources
        .iter()
        .find_map(|outcome| match outcome {
            ResourceOutcome::Failed(error) => Some(error.clone_for_selection()),
            ResourceOutcome::Unauthorized => Some(WorkBuddyError::TokenExpired),
            ResourceOutcome::Success(_) => None,
        })
        .or({
            if !usage_succeeded {
                Some(WorkBuddyError::NoData)
            } else {
                None
            }
        })
        .unwrap_or(WorkBuddyError::NoData)
}

impl WorkBuddyError {
    fn clone_for_selection(&self) -> Self {
        match self {
            Self::NotLoggedIn => Self::NotLoggedIn,
            Self::InvalidAuth => Self::InvalidAuth,
            Self::CredentialStorage => Self::CredentialStorage,
            Self::TokenExpired => Self::TokenExpired,
            Self::RefreshFailed => Self::RefreshFailed,
            Self::Connection => Self::Connection,
            Self::InvalidResponse => Self::InvalidResponse,
            Self::WafBlocked => Self::WafBlocked,
            Self::RequestFailed(status) => Self::RequestFailed(*status),
            Self::NoData => Self::NoData,
        }
    }
}

enum UsageFetchOutcome {
    Finished {
        pages: Vec<ParsedUsagePage>,
        complete: bool,
        total: usize,
    },
    Unauthorized {
        pages: Vec<ParsedUsagePage>,
        page_number: u32,
        total: usize,
    },
    Failed {
        pages: Vec<ParsedUsagePage>,
        total: usize,
        error: WorkBuddyError,
    },
}

fn latest_total(pages: &[ParsedUsagePage]) -> usize {
    pages.last().map(|page| page.total).unwrap_or(0)
}

fn build_snapshot(
    resources: Option<&MappedResources>,
    usage: UsageHistory,
    warnings: Vec<String>,
    now: DateTime<Local>,
) -> ProviderSnapshot {
    let (quotas, value_metrics) = resources.map(map_resource_metrics).unwrap_or_default();
    let credit_packages = resources
        .map(|resources| {
            resources
                .available_packages
                .iter()
                .map(|package| CreditPackage {
                    code: package.package_code.clone(),
                    name: package.package_name.clone(),
                    total: package.total,
                    remaining: package.remaining,
                    used: package.used,
                    expires_at: package.expire_at,
                })
                .collect()
        })
        .unwrap_or_default();
    let completeness = usage.completeness;
    let notices = match completeness {
        UsageCompleteness::Partial => vec![ProviderNotice {
            id: "workbuddy.usagePartial".into(),
            title: "Usage data is partial".into(),
            message: "Some WorkBuddy usage records could not be loaded.".into(),
            tone: ProviderNoticeTone::Warning,
        }],
        UsageCompleteness::Unavailable => vec![ProviderNotice {
            id: "workbuddy.usageUnavailable".into(),
            title: "Usage data unavailable".into(),
            message: "WorkBuddy balance is available, but usage details could not be loaded."
                .into(),
            tone: ProviderNoticeTone::Info,
        }],
        UsageCompleteness::Complete => Vec::new(),
    };
    ProviderSnapshot {
        credit_packages,
        provider_id: PROVIDER_ID.into(),
        plan: None,
        quotas,
        value_metrics,
        status_metrics: Vec::<StatusMetric>::new(),
        notices,
        usage,
        warnings,
        refreshed_at: now.with_timezone(&Utc),
    }
}

/// WorkBuddy exposes its credits as a remaining-balance value metric rather than a
/// consumed-versus-total quota: the snapshot contract requires every quota/value
/// metric to be referenced by the provider definition, and the aggregate
/// used/total split already lives in each credit package row. Returning no quota
/// keeps "Credits" a single, unambiguous number (可用积分).
fn map_resource_metrics(resources: &MappedResources) -> (Vec<QuotaWindow>, Vec<ValueMetric>) {
    let expiries_at = resources
        .available_packages
        .iter()
        .filter_map(|package| package.expire_at)
        .collect();
    let balance = ValueMetric {
        id: "balance".into(),
        label: "Balance".into(),
        values: vec![MetricValue {
            number: resources.remaining,
            kind: MetricValueKind::Count,
            label: None,
            estimated: false,
        }],
        expiries_at,
    };
    let nearest_expiring = ValueMetric {
        id: "nearestExpiring".into(),
        label: "近期到期的积分包".into(),
        values: vec![MetricValue {
            number: resources.nearest_expiring_remaining,
            kind: MetricValueKind::Count,
            label: None,
            estimated: false,
        }],
        expiries_at: resources.nearest_expiring_at.into_iter().collect(),
    };
    (Vec::new(), vec![balance, nearest_expiring])
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;
    use serde_json::json;

    use super::{
        classify_endpoint, definition, map_resource_metrics, EndpointResponse, ResourceOutcome,
        WorkBuddyError,
    };
    use crate::models::{MetricSection, MetricSource, ProviderErrorKind};
    use crate::providers::workbuddy::mapper::{MappedResources, ResourcePackage};
    use crate::providers::ProviderError;
    use chrono::{TimeZone, Utc};

    #[test]
    fn definition_exposes_credit_and_usage_metrics() {
        let definition = definition();
        assert_eq!(definition.id, "workbuddy");
        assert_eq!(definition.short_name, "WB");
        assert_eq!(definition.display_name, "Workbuddy CN");
        let nearest = definition
            .metrics
            .iter()
            .find(|metric| metric.id == "workbuddy.nearestExpiring")
            .expect("nearest expiring metric");
        assert_eq!(nearest.label, "近期到期的积分包");
        assert!(nearest.pinnable);
        assert_eq!(nearest.tray.as_ref().unwrap().suffix, None);
        let packages = definition
            .metrics
            .iter()
            .find(|metric| metric.id == "workbuddy.creditPackages")
            .expect("credit packages metric");
        assert_eq!(packages.source, MetricSource::CreditPackages);
        assert!(packages.default_enabled);
        assert_eq!(packages.default_section, MetricSection::AlwaysVisible);
        assert!(!packages.pinnable);
        assert!(definition
            .metrics
            .iter()
            .any(|metric| metric.id == "workbuddy.credits"));
        assert!(definition
            .metrics
            .iter()
            .any(|metric| metric.id == "workbuddy.trend"));
    }

    #[test]
    fn credits_metric_replaces_the_duplicate_balance_metric() {
        let definition = definition();
        let credits = definition
            .metrics
            .iter()
            .find(|metric| metric.id == "workbuddy.credits")
            .expect("credits metric");
        assert_eq!(
            credits.source,
            MetricSource::Value {
                source_id: "balance".into()
            }
        );
        assert!(credits.default_pinned);
        assert!(!definition
            .metrics
            .iter()
            .any(|metric| metric.id == "workbuddy.balance"));
    }

    #[test]
    fn http_forbidden_is_permission_instead_of_refreshable_unauthorized() {
        let outcome = classify_endpoint(Ok(EndpointResponse {
            status: StatusCode::FORBIDDEN,
            body: json!({"code": 403, "message": "permission denied"}),
        }));
        assert!(matches!(
            outcome,
            ResourceOutcome::Failed(WorkBuddyError::RequestFailed(403))
        ));

        let provider_error = ProviderError::from(WorkBuddyError::RequestFailed(403));
        assert_eq!(provider_error.kind(), ProviderErrorKind::Permission);

        let body_unauthorized = classify_endpoint(Ok(EndpointResponse {
            status: StatusCode::OK,
            body: json!({"code": 403, "message": "token expired"}),
        }));
        assert!(matches!(body_unauthorized, ResourceOutcome::Unauthorized));
    }

    #[test]
    fn mapped_metrics_are_all_exposed_by_the_definition() {
        let resources = MappedResources {
            packages: Vec::new(),
            total: 3315.0,
            remaining: 1019.42,
            used: 2295.58,
            available_packages: Vec::new(),
            nearest_expiring_remaining: 71.38,
            expired_remaining: 0.0,
            soonest_expire_at: resources_expiry(),
            nearest_expiring_at: resources_expiry(),
        };

        let (quotas, value_metrics) = map_resource_metrics(&resources);
        let definition = definition();
        let quota_sources = definition
            .metrics
            .iter()
            .filter_map(|metric| match &metric.source {
                MetricSource::Quota { source_id, .. }
                | MetricSource::QuotaOrValue { source_id, .. } => Some(source_id.as_str()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        let value_sources = definition
            .metrics
            .iter()
            .filter_map(|metric| match &metric.source {
                MetricSource::Value { source_id } => Some(source_id.as_str()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();

        assert!(
            quotas
                .iter()
                .all(|quota| quota_sources.contains(quota.id.as_str())),
            "snapshot quotas must be referenced by the provider definition: {quotas:?}"
        );
        assert!(
            value_metrics
                .iter()
                .all(|metric| value_sources.contains(metric.id.as_str())),
            "snapshot value metrics must be referenced by the provider definition"
        );
    }

    #[test]
    fn balance_value_omits_the_redundant_credit_unit_label() {
        let resources = MappedResources {
            packages: Vec::new(),
            total: 3315.0,
            remaining: 1019.42,
            used: 2295.58,
            available_packages: Vec::new(),
            nearest_expiring_remaining: 71.38,
            expired_remaining: 0.0,
            soonest_expire_at: resources_expiry(),
            nearest_expiring_at: resources_expiry(),
        };

        let (_, value_metrics) = map_resource_metrics(&resources);
        let balance = value_metrics
            .iter()
            .find(|metric| metric.id == "balance")
            .expect("balance metric");

        assert_eq!(balance.values[0].label, None);
    }

    #[test]
    fn balance_expiries_only_include_current_credit_packages() {
        let historical = ResourcePackage {
            package_code: "historical".into(),
            package_name: "历史积分包".into(),
            total: 100.0,
            remaining: 0.0,
            used: 100.0,
            status: Some(0),
            expire_at: Some(Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()),
        };
        let current = ResourcePackage {
            package_code: "current".into(),
            package_name: "当前积分包".into(),
            total: 100.0,
            remaining: 71.38,
            used: 28.62,
            status: Some(0),
            expire_at: Some(Utc.with_ymd_and_hms(2026, 10, 8, 15, 59, 59).unwrap()),
        };
        let resources = MappedResources {
            packages: vec![historical, current.clone()],
            total: 100.0,
            remaining: 71.38,
            used: 28.62,
            available_packages: vec![current],
            nearest_expiring_remaining: 71.38,
            expired_remaining: 0.0,
            soonest_expire_at: resources_expiry(),
            nearest_expiring_at: resources_expiry(),
        };

        let (_, value_metrics) = map_resource_metrics(&resources);
        let balance = value_metrics
            .iter()
            .find(|metric| metric.id == "balance")
            .expect("balance metric");
        let nearest = value_metrics
            .iter()
            .find(|metric| metric.id == "nearestExpiring")
            .expect("nearest expiring metric");

        assert_eq!(balance.expiries_at, vec![resources_expiry().unwrap()]);
        assert_eq!(nearest.values[0].label, None);
    }

    fn resources_expiry() -> Option<chrono::DateTime<Utc>> {
        Some(Utc.with_ymd_and_hms(2026, 10, 8, 15, 59, 59).unwrap())
    }
}
