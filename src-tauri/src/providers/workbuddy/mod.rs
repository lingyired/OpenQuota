mod auth;
mod client;
mod login;
#[cfg(test)]
mod login_tests;
mod mapper;
mod session;
mod usage;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Days, Local, TimeZone, Utc};
use serde_json::Value;
use thiserror::Error;

use crate::models::{
    ApiKeyStatus, CreditPackage, DeviceCodeChallenge, DeviceCodePoll, MetricDefinition,
    MetricSection, MetricSource, MetricValue, MetricValueKind, ProviderDefinition,
    ProviderErrorKind, ProviderLink, ProviderNotice, ProviderNoticeTone, ProviderSnapshot,
    QuotaWindow, StatusMetric, UsageCompleteness, UsageHistory, UsagePeriodSelection, ValueMetric,
};

use self::{
    auth::{WorkBuddyAuth, WorkBuddyAuthError},
    client::{EndpointResponse, WorkBuddyClient, WorkBuddyClientError},
    login::{DeviceCodeLogin, LoginPoll, WorkBuddyLoginError},
    mapper::{map_resources, MappedResources},
    session::{WorkBuddySession, WorkBuddySessionError, WorkBuddySessionStore},
    usage::{build_history, collect_pages, parse_page, ParsedUsagePage, MAX_PAGES},
};
use super::{
    DeviceCodeAuth, ProviderError, ProviderRefresh, ProviderRequestContext, UsageProvider,
};

const PROVIDER_ID: &str = "workbuddy-cn";
const SOURCE_NOTE: &str = "WorkBuddy official usage";
/// Quota01 自己签约凭据走 CN 站点：设备码流程就是为这个站点实现的。
const DEVICE_CODE_BASE_URL: &str = "https://www.codebuddy.cn";

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
            MetricDefinition::new(
                "workbuddy-cn.nearestExpiring",
                "近期到期的积分包",
                MetricSource::NearestCreditPackage {
                    source_id: "nearestExpiring".into(),
                },
                true,
                true,
                MetricSection::AlwaysVisible,
                false,
                Some("近"),
                None,
            ),
            MetricDefinition::value(
                "workbuddy-cn.credits",
                "Credits",
                "balance",
                true,
                MetricSection::AlwaysVisible,
                true,
                "C",
                None,
            ),
            MetricDefinition::new(
                "workbuddy-cn.creditPackages",
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
                "workbuddy-cn.today",
                "Today",
                UsagePeriodSelection::Today,
                MetricSection::OnDemand,
                "T",
            ),
            MetricDefinition::usage(
                "workbuddy-cn.yesterday",
                "Yesterday",
                UsagePeriodSelection::Yesterday,
                MetricSection::OnDemand,
                "Y",
            ),
            MetricDefinition::usage(
                "workbuddy-cn.last30",
                "Last 30 Days",
                UsagePeriodSelection::Last30Days,
                MetricSection::OnDemand,
                "M",
            ),
            MetricDefinition::trend("workbuddy-cn.trend"),
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
    #[error("WorkBuddy 5.6 encrypts the login data it keeps on this computer, so it cannot be read directly. Sign in to WorkBuddy from Quota01 to connect; the legacy plaintext login file is still used when present.")]
    CredentialsEncrypted,
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
            WorkBuddyAuthError::Encrypted => Self::CredentialsEncrypted,
        }
    }
}

impl From<WorkBuddySessionError> for WorkBuddyError {
    fn from(error: WorkBuddySessionError) -> Self {
        match error {
            // 会话文档损坏等同于登录数据无效：两者都只能靠重新登录解决。
            WorkBuddySessionError::Malformed => Self::InvalidAuth,
            WorkBuddySessionError::Storage => Self::CredentialStorage,
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

impl From<WorkBuddyLoginError> for WorkBuddyError {
    fn from(error: WorkBuddyLoginError) -> Self {
        match error {
            WorkBuddyLoginError::Connection => Self::Connection,
            WorkBuddyLoginError::InvalidResponse => Self::InvalidResponse,
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
            WorkBuddyError::CredentialStorage | WorkBuddyError::CredentialsEncrypted => {
                ProviderErrorKind::CredentialStorage
            }
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

/// 缓存身份必须跟着真正生效的凭据来源走，否则会话与登录文件并存时
/// 会把两个账号的缓存混在一起。读不到凭据时保持未解析，
/// 与原实现忽略登录错误的行为一致。
fn resolve_identity(credential: Result<WorkBuddyCredential, WorkBuddyError>) -> Option<String> {
    credential.ok().and_then(|credential| {
        credential
            .uid()
            .map(|uid| crate::hashing::sha256_hex(uid.as_bytes()))
    })
}

/// 凭据来源与 provider 实例无关，抽成自由函数让 `new` 在构造实例前也能解析缓存身份。
fn load_credentials_at(
    sessions: &WorkBuddySessionStore,
    auth_path: &Path,
) -> Result<WorkBuddyCredential, WorkBuddyError> {
    match sessions.load()? {
        Some(session) => Ok(WorkBuddyCredential::Session(session)),
        None => Ok(WorkBuddyCredential::AuthFile(
            WorkBuddyAuth::load_from_path(auth_path)?,
        )),
    }
}

pub struct WorkBuddyProvider {
    client: Arc<WorkBuddyClient>,
    account_identity: Option<String>,
    /// 凭据来源路径由 provider 持有，测试才能指向临时登录文件而不是用户真实的那份。
    auth_path: PathBuf,
    /// Quota01 自己签发的会话。它是首选凭据，刷新结果也只写回这里。
    sessions: WorkBuddySessionStore,
    /// 设备码登录状态机：Quota01 自己签发凭据的入口，`state` 只留在它内部。
    login: DeviceCodeLogin,
    /// 测试用的临时登录目录，随 provider 一起析构；生产构造下为 None。
    #[cfg(test)]
    _auth_dir: Option<tempfile::TempDir>,
}

/// 凭据来源：Quota01 自己登录得到的会话，或 WorkBuddy 旧版留下的明文登录文件。
/// 刷新结果只会写回来源本身，两个来源之间绝不交叉落库。
enum WorkBuddyCredential {
    /// Quota01 自己登录得到的会话：可以刷新并写回 vault。
    Session(WorkBuddySession),
    /// WorkBuddy 老版本留下的明文登录文件：沿用原有的原子写回。
    AuthFile(WorkBuddyAuth),
}

impl WorkBuddyCredential {
    /// client 只认 `&WorkBuddyAuth`：两种来源在这里统一成同一种视图，
    /// 请求头与 host 选择因此与改动前完全一致。
    fn view(&self) -> WorkBuddyAuth {
        match self {
            Self::Session(session) => WorkBuddyAuth::from_session(session),
            Self::AuthFile(auth) => auth.clone(),
        }
    }

    fn uid(&self) -> Option<&str> {
        match self {
            Self::Session(session) => session.uid.as_deref(),
            Self::AuthFile(auth) => auth.uid.as_deref(),
        }
    }

    fn has_refresh_token(&self) -> bool {
        self.refresh_token()
            .is_some_and(|token| !token.trim().is_empty())
    }

    fn refresh_token(&self) -> Option<&str> {
        match self {
            Self::Session(session) => session.refresh_token.as_deref(),
            Self::AuthFile(auth) => auth.refresh_token.as_deref(),
        }
    }

    /// 刷新结果只回写到凭据来源本身：会话进 vault，登录文件走原子写回。
    /// 两条分支互不调用对方的写入口，所以会话凭据不可能被写进登录文件，
    /// 登录文件凭据也不可能被写进 vault。
    fn persist_refresh(
        &mut self,
        sessions: &WorkBuddySessionStore,
        access_token: String,
        refresh_token: Option<String>,
    ) -> Result<(), WorkBuddyError> {
        match self {
            Self::Session(session) => {
                session.access_token = access_token;
                if refresh_token.is_some() {
                    session.refresh_token = refresh_token;
                }
                sessions.save(&session.to_json()?)?;
            }
            Self::AuthFile(auth) => {
                if let Err(error) = auth.save_tokens(access_token.clone(), refresh_token.clone()) {
                    // 登录文件写不进去时仍然采用新 token 完成本次刷新，行为与改动前一致。
                    auth.access_token = access_token;
                    if refresh_token.is_some() {
                        auth.refresh_token = refresh_token;
                    }
                    return Err(error.into());
                }
            }
        }
        Ok(())
    }
}

impl WorkBuddyProvider {
    pub fn new() -> Result<Self, WorkBuddyError> {
        let auth_path = auth::auth_file_path();
        let sessions = WorkBuddySessionStore::new();
        let account_identity = resolve_identity(load_credentials_at(&sessions, &auth_path));
        Ok(Self {
            client: Arc::new(WorkBuddyClient::new()?),
            account_identity,
            auth_path,
            sessions,
            login: DeviceCodeLogin::new(DEVICE_CODE_BASE_URL)?,
            #[cfg(test)]
            _auth_dir: None,
        })
    }

    /// 测试构造：会话走内存 vault，登录文件走临时目录，两者都不碰用户真实数据。
    /// `file_token == Some("ENCRYPTED")` 写成 WorkBuddy 5.6 的加密信封。
    #[cfg(test)]
    pub(crate) fn for_test_with_session(
        session_token: Option<&str>,
        file_token: Option<&str>,
    ) -> Self {
        let dir = tempfile::tempdir().expect("test auth dir");
        let auth_path = dir.path().join("workbuddy-desktop.info");
        match file_token {
            // 哨兵值：auth 层必须把它判成 Encrypted，而不是「未登录」。
            Some("ENCRYPTED") => std::fs::write(
                &auth_path,
                r#"{"account":{"uid":"uid-file"},"auth":{"accessToken":{"$wbEncrypted":1,"envelope":"ZW5j"},"tokenType":"Bearer","domain":"www.codebuddy.cn"}}"#,
            )
            .expect("write envelope"),
            Some(token) => std::fs::write(
                &auth_path,
                format!(
                    r#"{{"auth":{{"accessToken":"{token}","refreshToken":"refresh-file"}},"domain":"www.codebuddy.cn","uid":"uid-file"}}"#
                ),
            )
            .expect("write login file"),
            None => {}
        }
        let sessions = tests::memory_session_store(
            session_token
                .map(|token| tests::session_document(token, None))
                .as_deref(),
        );
        let account_identity = resolve_identity(load_credentials_at(&sessions, &auth_path));
        Self {
            // 默认指向不可路由的地址：任何忘记覆盖 base URL 的测试都不可能打到真实 WorkBuddy。
            client: Arc::new(WorkBuddyClient::for_test("http://127.0.0.1:1")),
            account_identity,
            auth_path,
            sessions,
            // 登录状态机同样指向不可路由地址，测试只能通过 `with_test_base_url` 指到本地 server。
            login: DeviceCodeLogin::for_test("http://127.0.0.1:1"),
            _auth_dir: Some(dir),
        }
    }

    /// 把请求指向本地测试 server；凭据来源仍由 `for_test_with_session` 决定。
    #[cfg(test)]
    pub(crate) fn with_test_base_url(mut self, base_url: &str) -> Self {
        self.client = Arc::new(WorkBuddyClient::for_test(base_url));
        // 登录状态机与业务请求必须打到同一台测试 server，否则轮询会打到已关闭的端口。
        self.login = DeviceCodeLogin::for_test(base_url);
        self
    }

    /// 首选 Quota01 自己的会话，其次才是旧版明文登录文件。
    ///
    /// 已过期的会话也照常返回：让请求先走 unauthorized→refresh 续期，只有刷新失败
    /// 才以 `TokenExpired` 收场；在这里按本地时间丢弃会把「可续期的登录」误报成「未登录」。
    fn load_credentials(&self) -> Result<WorkBuddyCredential, WorkBuddyError> {
        load_credentials_at(&self.sessions, &self.auth_path)
    }

    fn refresh_with_identity(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<(ProviderSnapshot, Option<String>), WorkBuddyError> {
        let now = Local::now();
        let mut credential = self.load_credentials()?;
        let account_identity = credential
            .uid()
            .map(|uid| crate::hashing::sha256_hex(uid.as_bytes()));
        let mut warnings = Vec::new();
        let mut refresh_attempted = false;

        let mut resources = self.fetch_resources(context, &credential.view(), now);
        if resources.iter().any(ResourceOutcome::is_unauthorized) {
            if credential.has_refresh_token() {
                refresh_attempted = true;
                match self.refresh_auth_with_context(context, &mut credential, &mut warnings) {
                    Ok(()) => {
                        resources = self.retry_unauthorized_resources(
                            context,
                            &credential.view(),
                            now,
                            resources,
                        );
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
        let usage_attempt =
            self.fetch_usage_pages(context, &credential.view(), start, now, Vec::new(), 1);
        let (usage, usage_succeeded) = self.finish_usage(
            context,
            &mut credential,
            UsageWindow { start, end: now },
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

    fn fetch_resources(
        &self,
        context: &ProviderRequestContext,
        auth: &WorkBuddyAuth,
        now: DateTime<Local>,
    ) -> ResourceOutcomes {
        let (summary, paid, free) = std::thread::scope(|scope| {
            let summary = scope.spawn(|| {
                self.client
                    .fetch_resource_summary_with_context(context, auth, now)
            });
            let paid = scope.spawn(|| {
                self.client
                    .fetch_paid_packages_with_context(context, auth, now)
            });
            let free = scope.spawn(|| {
                self.client
                    .fetch_free_packages_with_context(context, auth, now)
            });
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
        context: &ProviderRequestContext,
        auth: &WorkBuddyAuth,
        now: DateTime<Local>,
        resources: ResourceOutcomes,
    ) -> ResourceOutcomes {
        let [summary, paid, free] = resources.0;
        ResourceOutcomes([
            retry_resource(summary, || {
                self.client
                    .fetch_resource_summary_with_context(context, auth, now)
            }),
            retry_resource(paid, || {
                self.client
                    .fetch_paid_packages_with_context(context, auth, now)
            }),
            retry_resource(free, || {
                self.client
                    .fetch_free_packages_with_context(context, auth, now)
            }),
        ])
    }

    #[cfg(test)]
    fn refresh_auth(
        &self,
        credential: &mut WorkBuddyCredential,
        warnings: &mut Vec<String>,
    ) -> Result<(), WorkBuddyError> {
        self.refresh_auth_with_context(
            &ProviderRequestContext::direct(Arc::default()),
            credential,
            warnings,
        )
    }

    fn refresh_auth_with_context(
        &self,
        context: &ProviderRequestContext,
        credential: &mut WorkBuddyCredential,
        warnings: &mut Vec<String>,
    ) -> Result<(), WorkBuddyError> {
        if !credential.has_refresh_token() {
            return Err(WorkBuddyError::TokenExpired);
        }
        let (access_token, refresh_token) = self
            .client
            .refresh_token_with_context(context, &credential.view())?;
        if let Err(error) = credential.persist_refresh(&self.sessions, access_token, refresh_token)
        {
            warnings.push(format!(
                "WorkBuddy token was refreshed for this session but could not be saved: {error}"
            ));
        }
        Ok(())
    }

    fn fetch_usage_pages(
        &self,
        context: &ProviderRequestContext,
        auth: &WorkBuddyAuth,
        start: DateTime<Local>,
        end: DateTime<Local>,
        mut pages: Vec<ParsedUsagePage>,
        mut page_number: u32,
    ) -> UsageFetchOutcome {
        loop {
            let response = match self.client.fetch_usage_page_with_context(
                context,
                auth,
                start,
                end,
                page_number,
            ) {
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
        context: &ProviderRequestContext,
        credential: &mut WorkBuddyCredential,
        window: UsageWindow,
        attempt: UsageFetchOutcome,
        refresh_attempted: &mut bool,
        warnings: &mut Vec<String>,
    ) -> (UsageHistory, bool) {
        let attempt = if let UsageFetchOutcome::Unauthorized {
            pages, page_number, ..
        } = attempt
        {
            if !*refresh_attempted && credential.has_refresh_token() {
                *refresh_attempted = true;
                match self.refresh_auth_with_context(context, credential, warnings) {
                    Ok(()) => self.fetch_usage_pages(
                        context,
                        &credential.view(),
                        window.start,
                        window.end,
                        pages,
                        page_number,
                    ),
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
                let history = build_history(&collection, window.end, SOURCE_NOTE);
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
                    let history = build_history(&collection, window.end, SOURCE_NOTE);
                    append_usage_warning(history.completeness, warnings);
                    (history, true)
                }
            }
        }
    }

    pub fn refresh(&self) -> Result<ProviderSnapshot, WorkBuddyError> {
        self.refresh_with_identity(&ProviderRequestContext::direct(Arc::default()))
            .map(|(snapshot, _)| snapshot)
    }
}

impl UsageProvider for WorkBuddyProvider {
    fn definition(&self) -> ProviderDefinition {
        definition()
    }

    fn has_local_credentials(&self) -> bool {
        // 设备码登录不需要机器上装过 WorkBuddy，所以只看登录文件会把「已经用 Quota01
        // 登录过、但没装 WorkBuddy」的用户判成 Absent，从而自动禁用并隐藏 provider。
        // 这里沿用 `ApiKeyStore::has_credentials` 的容错：vault 读不出来时按无会话处理。
        WorkBuddyAuth::has_local_credentials_at(&self.auth_path)
            || self
                .sessions
                .status()
                .is_ok_and(|status| status != ApiKeyStatus::NotSet)
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

    fn device_code_auth(&self) -> Option<DeviceCodeAuth> {
        Some(DeviceCodeAuth {
            platform: "workbuddy".into(),
        })
    }

    /// 申请 state 的两种失败要分开：连不上是网络问题，响应不可信是服务端问题，
    /// 前端据此给用户不同的提示。
    fn start_device_code_login(&self) -> Result<DeviceCodeChallenge, ProviderError> {
        self.start_device_code_login_with_context(&ProviderRequestContext::direct(Arc::default()))
    }

    fn start_device_code_login_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<DeviceCodeChallenge, ProviderError> {
        self.login.start_with_context(context).map_err(|error| {
            let kind = match error {
                WorkBuddyLoginError::Connection => ProviderErrorKind::Network,
                WorkBuddyLoginError::InvalidResponse => ProviderErrorKind::InvalidResponse,
            };
            ProviderError::from_display(kind, error)
        })
    }

    /// 轮询结果先在 provider 内部落库，再换成不含凭据的 wire 类型回传。
    /// 落库失败时这次尝试同样已经结束（done 为 true），但必须把失败报成错误文案，
    /// 不能让前端以为凭据已经可用。
    fn poll_device_code_login(&self, login_id: &str) -> DeviceCodePoll {
        self.poll_device_code_login_with_context(
            login_id,
            &ProviderRequestContext::direct(Arc::default()),
        )
    }

    fn poll_device_code_login_with_context(
        &self,
        login_id: &str,
        context: &ProviderRequestContext,
    ) -> DeviceCodePoll {
        match self.login.poll_with_context(login_id, context) {
            LoginPoll::Pending => DeviceCodePoll {
                done: false,
                error: None,
            },
            LoginPoll::Failed(message) => DeviceCodePoll {
                done: true,
                error: Some(message),
            },
            LoginPoll::Ready(session) => {
                // 存的是完整会话文档，不是裸 token：store 会按会话文档校验。
                let saved = session
                    .to_json()
                    .and_then(|document| self.sessions.save(&document));
                match saved {
                    Ok(()) => DeviceCodePoll {
                        done: true,
                        error: None,
                    },
                    Err(error) => DeviceCodePoll {
                        done: true,
                        error: Some(error.to_string()),
                    },
                }
            }
        }
    }

    fn cancel_device_code_login(&self, login_id: &str) -> bool {
        self.login.cancel(login_id)
    }

    fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
        WorkBuddyProvider::refresh(self).map_err(ProviderError::from)
    }

    fn refresh_for_service(&self) -> Result<ProviderRefresh, ProviderError> {
        self.refresh_for_service_with_context(&ProviderRequestContext::direct(Arc::default()))
    }

    fn refresh_for_service_with_context(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<ProviderRefresh, ProviderError> {
        let (snapshot, identity) = self
            .refresh_with_identity(context)
            .map_err(ProviderError::from)?;
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

    /// 只报告 Quota01 自己签发的会话。旧版明文登录文件是 Quota01 不拥有的凭据，
    /// 而这个面板的意义正是提供 Quota01 自己的登录入口，所以它不算作一个会话。
    fn session_status(&self) -> Option<Result<ApiKeyStatus, ProviderError>> {
        Some(
            self.sessions
                .status()
                .map_err(WorkBuddyError::from)
                .map_err(ProviderError::from),
        )
    }

    fn save_session(&self, value: &str) -> Result<(), ProviderError> {
        self.sessions
            .save(value)
            .map_err(WorkBuddyError::from)
            .map_err(ProviderError::from)
    }

    fn delete_session(&self) -> Result<(), ProviderError> {
        // 只清 vault 里的会话：登录文件属于 WorkBuddy 自己，绝不在这里删除。
        self.sessions
            .delete()
            .map_err(WorkBuddyError::from)
            .map_err(ProviderError::from)
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
            Self::CredentialsEncrypted => Self::CredentialsEncrypted,
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

struct UsageWindow {
    start: DateTime<Local>,
    end: DateTime<Local>,
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
                    unlimited: false,
                })
                .collect()
        })
        .unwrap_or_default();
    let completeness = usage.completeness;
    let notices = match completeness {
        UsageCompleteness::Partial => vec![ProviderNotice {
            id: "workbuddy-cn.usagePartial".into(),
            title: "Usage data is partial".into(),
            message: "Some WorkBuddy usage records could not be loaded.".into(),
            tone: ProviderNoticeTone::Warning,
        }],
        UsageCompleteness::Unavailable => vec![ProviderNotice {
            id: "workbuddy-cn.usageUnavailable".into(),
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
            label: Some("credits".into()),
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
            label: Some("credits".into()),
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

    use std::sync::{Arc, Mutex};

    use super::{
        classify_endpoint, definition, map_resource_metrics, EndpointResponse, ResourceOutcome,
        WorkBuddyCredential, WorkBuddyError, WorkBuddyProvider,
    };
    use crate::models::{ApiKeyStatus, MetricSection, MetricSource, ProviderErrorKind};
    use crate::providers::api_key::{ApiKeyStore, EnvironmentReader, SecretBackend, SecretBytes};
    use crate::providers::test_http;
    use crate::providers::workbuddy::mapper::{MappedResources, ResourcePackage};
    use crate::providers::workbuddy::session::WorkBuddySessionStore;
    use crate::providers::{ProviderError, UsageProvider};
    use chrono::{TimeZone, Utc};

    #[test]
    fn definition_exposes_credit_and_usage_metrics() {
        let definition = definition();
        assert_eq!(definition.id, "workbuddy-cn");
        assert_eq!(definition.short_name, "WB");
        assert_eq!(definition.display_name, "Workbuddy CN");
        assert_eq!(definition.metrics[0].id, "workbuddy-cn.nearestExpiring");
        let nearest = definition
            .metrics
            .iter()
            .find(|metric| metric.id == "workbuddy-cn.nearestExpiring")
            .expect("nearest expiring metric");
        assert_eq!(nearest.label, "近期到期的积分包");
        assert!(nearest.pinnable);
        assert_eq!(
            nearest.source,
            MetricSource::NearestCreditPackage {
                source_id: "nearestExpiring".into()
            }
        );
        assert_eq!(nearest.tray.as_ref().unwrap().suffix, None);
        let packages = definition
            .metrics
            .iter()
            .find(|metric| metric.id == "workbuddy-cn.creditPackages")
            .expect("credit packages metric");
        assert_eq!(packages.source, MetricSource::CreditPackages);
        assert!(packages.default_enabled);
        assert_eq!(packages.default_section, MetricSection::AlwaysVisible);
        assert!(!packages.pinnable);
        assert!(definition
            .metrics
            .iter()
            .any(|metric| metric.id == "workbuddy-cn.credits"));
        assert!(definition
            .metrics
            .iter()
            .any(|metric| metric.id == "workbuddy-cn.trend"));
    }

    #[test]
    fn credits_metric_replaces_the_duplicate_balance_metric() {
        let definition = definition();
        let credits = definition
            .metrics
            .iter()
            .find(|metric| metric.id == "workbuddy-cn.credits")
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
            .any(|metric| metric.id == "workbuddy-cn.balance"));
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
    fn encrypted_local_credentials_do_not_masquerade_as_signed_out() {
        let encrypted = ProviderError::from(WorkBuddyError::CredentialsEncrypted);
        assert_eq!(encrypted.kind(), ProviderErrorKind::CredentialStorage);

        let message = WorkBuddyError::CredentialsEncrypted.to_string();
        assert_ne!(message, WorkBuddyError::NotLoggedIn.to_string());
        assert!(message.contains("5.6"));
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
                MetricSource::Value { source_id }
                | MetricSource::NearestCreditPackage { source_id } => Some(source_id.as_str()),
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
    fn balance_value_carries_the_credit_unit_the_marker_keys_off() {
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

        assert_eq!(balance.values[0].label.as_deref(), Some("credits"));
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
        assert_eq!(nearest.values[0].label.as_deref(), Some("credits"));
    }

    fn resources_expiry() -> Option<chrono::DateTime<Utc>> {
        Some(Utc.with_ymd_and_hms(2026, 10, 8, 15, 59, 59).unwrap())
    }

    struct MemorySecrets(Mutex<Option<Vec<u8>>>);

    impl SecretBackend for MemorySecrets {
        fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
            Ok(self.0.lock().unwrap().clone().map(SecretBytes::new))
        }

        fn write(&self, _account: &str, value: &[u8]) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(value.to_vec());
            Ok(())
        }

        fn delete(&self, _account: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    struct EmptyEnvironment;

    impl EnvironmentReader for EmptyEnvironment {
        fn value(&self, _name: &str) -> Option<String> {
            None
        }
    }

    /// 内存后端，绝不触碰用户真实的凭证 vault（沿用 deepseek / trae 测试的做法）。
    pub(super) fn memory_session_store(value: Option<&str>) -> WorkBuddySessionStore {
        let secrets = Arc::new(MemorySecrets(Mutex::new(
            value.map(|text| text.as_bytes().to_vec()),
        )));
        WorkBuddySessionStore::with_store(ApiKeyStore::with_backends(
            "workbuddy-cn-session",
            "WORKBUDDY_SESSION",
            secrets,
            Arc::new(EmptyEnvironment),
        ))
    }

    /// 写入永远失败的 vault：登录成功但会话存不下去时，轮询必须把这件事报回前端，
    /// 而不是静默当成登录成功。
    struct FailingSecrets;

    impl SecretBackend for FailingSecrets {
        fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
            Ok(None)
        }

        fn write(&self, _account: &str, _value: &[u8]) -> Result<(), String> {
            Err("vault unavailable".into())
        }

        fn delete(&self, _account: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn failing_session_store() -> WorkBuddySessionStore {
        WorkBuddySessionStore::with_store(ApiKeyStore::with_backends(
            "workbuddy-cn-session",
            "WORKBUDDY_SESSION",
            Arc::new(FailingSecrets),
            Arc::new(EmptyEnvironment),
        ))
    }

    /// 测试用的会话文档：带 refresh_token，刷新写回才有东西可续。
    pub(super) fn session_document(access_token: &str, expires_at: Option<i64>) -> String {
        json!({
            "access_token": access_token,
            "refresh_token": "refresh-session",
            "domain": "www.codebuddy.cn",
            "uid": "uid-session",
            "expires_at": expires_at,
        })
        .to_string()
    }

    #[test]
    fn a_stored_session_wins_over_the_login_file() {
        let provider = WorkBuddyProvider::for_test_with_session(Some("access-session"), None);
        match provider.load_credentials().unwrap() {
            WorkBuddyCredential::Session(session) => {
                assert_eq!(session.access_token, "access-session")
            }
            WorkBuddyCredential::AuthFile(_) => panic!("the stored session must win"),
        }
    }

    #[test]
    fn the_stored_session_wins_even_when_the_login_file_also_exists() {
        let provider =
            WorkBuddyProvider::for_test_with_session(Some("access-session"), Some("access-file"));
        match provider.load_credentials().unwrap() {
            WorkBuddyCredential::Session(session) => {
                assert_eq!(session.access_token, "access-session")
            }
            WorkBuddyCredential::AuthFile(_) => panic!("the stored session must win"),
        }
    }

    #[test]
    fn the_login_file_is_used_when_no_session_is_stored() {
        let provider = WorkBuddyProvider::for_test_with_session(None, Some("access-file"));
        match provider.load_credentials().unwrap() {
            WorkBuddyCredential::AuthFile(auth) => assert_eq!(auth.access_token, "access-file"),
            WorkBuddyCredential::Session(_) => panic!("the login file must be the fallback"),
        }
    }

    #[test]
    fn an_encrypted_login_file_without_a_session_reports_encryption() {
        let provider = WorkBuddyProvider::for_test_with_session(None, Some("ENCRYPTED"));
        assert!(matches!(
            provider.load_credentials(),
            Err(WorkBuddyError::CredentialsEncrypted)
        ));
    }

    #[test]
    fn an_expired_stored_session_is_still_returned_so_refresh_can_renew_it() {
        let provider = WorkBuddyProvider::for_test_with_session(None, None);
        provider
            .sessions
            .save(&session_document("access-expired", Some(1_000)))
            .unwrap();

        match provider.load_credentials().unwrap() {
            WorkBuddyCredential::Session(session) => {
                assert_eq!(session.access_token, "access-expired");
                assert_eq!(session.expires_at, Some(1_000));
            }
            WorkBuddyCredential::AuthFile(_) => {
                panic!("an expired session must still win so the refresh path can renew it")
            }
        }
    }

    #[test]
    fn a_refreshed_session_is_written_back_to_the_vault_and_never_to_the_login_file() {
        let server = test_http::serve_once(
            200,
            &[],
            r#"{"code":0,"data":{"accessToken":"refreshed-access","refreshToken":"refreshed-refresh"}}"#,
        );
        let provider = WorkBuddyProvider::for_test_with_session(Some("access-session"), None)
            .with_test_base_url(&server);
        let mut credential = provider.load_credentials().unwrap();
        let mut warnings = Vec::new();

        provider
            .refresh_auth(&mut credential, &mut warnings)
            .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");

        let stored = provider.sessions.load().unwrap().expect("stored session");
        assert_eq!(stored.access_token, "refreshed-access");
        assert_eq!(stored.refresh_token.as_deref(), Some("refreshed-refresh"));
        assert!(
            !provider.auth_path.exists(),
            "a Quota01 session must never be written into the WorkBuddy login file"
        );
    }

    #[test]
    fn a_refreshed_login_file_credential_stays_out_of_the_vault() {
        let server = test_http::serve_once(
            200,
            &[],
            r#"{"code":0,"data":{"accessToken":"refreshed-access","refreshToken":"refreshed-refresh"}}"#,
        );
        let provider = WorkBuddyProvider::for_test_with_session(None, Some("access-file"))
            .with_test_base_url(&server);
        let mut credential = provider.load_credentials().unwrap();
        let mut warnings = Vec::new();

        provider
            .refresh_auth(&mut credential, &mut warnings)
            .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");

        let document: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&provider.auth_path).unwrap()).unwrap();
        assert_eq!(document["auth"]["accessToken"], "refreshed-access");
        assert_eq!(document["auth"]["refreshToken"], "refreshed-refresh");
        assert!(
            provider.sessions.load().unwrap().is_none(),
            "a login-file credential must never be written into the vault"
        );
    }

    #[test]
    fn a_stored_session_is_reported_and_cleared_through_the_session_hooks() {
        let provider = WorkBuddyProvider::for_test_with_session(Some("access-session"), None);
        assert_eq!(
            provider.session_status().unwrap().unwrap(),
            ApiKeyStatus::Saved
        );

        provider.delete_session().unwrap();
        assert_eq!(
            provider.session_status().unwrap().unwrap(),
            ApiKeyStatus::NotSet
        );
    }

    #[test]
    fn a_provider_without_a_login_file_still_counts_as_having_local_credentials() {
        let with_session = WorkBuddyProvider::for_test_with_session(Some("access-session"), None);
        assert!(with_session.has_local_credentials());

        let neither = WorkBuddyProvider::for_test_with_session(None, None);
        assert!(!neither.has_local_credentials());
    }

    #[test]
    fn device_code_auth_reports_the_workbuddy_platform() {
        let provider = WorkBuddyProvider::for_test_with_session(None, None);

        let auth = provider
            .device_code_auth()
            .expect("WorkBuddy signs in with a device code");

        assert_eq!(auth.platform, "workbuddy");
    }

    #[test]
    fn starting_a_device_code_login_returns_the_challenge_from_the_test_server() {
        let server = test_http::serve_once(
            200,
            &[],
            &json!({"code": 0, "data": {"state": "st-1", "authUrl": "https://example.test/auth"}})
                .to_string(),
        );
        let provider =
            WorkBuddyProvider::for_test_with_session(None, None).with_test_base_url(&server);

        let challenge = provider
            .start_device_code_login()
            .expect("the test server issues a state");

        assert_eq!(challenge.verification_uri, "https://example.test/auth");
        assert!(!challenge.login_id.is_empty());
    }

    #[test]
    fn a_device_code_login_that_cannot_reach_workbuddy_maps_to_a_network_error() {
        let provider = WorkBuddyProvider::for_test_with_session(None, None);

        let error = provider
            .start_device_code_login()
            .expect_err("no server is reachable");

        assert_eq!(error.kind(), ProviderErrorKind::Network);
    }

    #[test]
    fn cancelling_a_device_code_login_reports_whether_it_was_active() {
        let provider = WorkBuddyProvider::for_test_with_session(None, None);
        let login_id = provider.login.register_for_test("st-1");

        assert!(provider.cancel_device_code_login(&login_id));
        assert!(!provider.cancel_device_code_login(&login_id));
    }

    #[test]
    fn a_pending_device_code_poll_reports_not_done_and_stores_nothing() {
        let server = test_http::serve_once(
            200,
            &[],
            &json!({"code": 12153, "msg": "pending"}).to_string(),
        );
        let provider =
            WorkBuddyProvider::for_test_with_session(None, None).with_test_base_url(&server);
        let login_id = provider.login.register_for_test("st-1");

        let poll = provider.poll_device_code_login(&login_id);

        assert!(!poll.done);
        assert_eq!(poll.error, None);
        assert!(provider.sessions.load().unwrap().is_none());
    }

    #[test]
    fn an_unknown_device_code_login_reports_done_with_a_message() {
        let provider = WorkBuddyProvider::for_test_with_session(None, None);

        let poll = provider.poll_device_code_login("missing");

        assert!(poll.done);
        let message = poll.error.expect("a failed attempt carries a message");
        assert!(!message.is_empty());
        assert!(provider.sessions.load().unwrap().is_none());
    }

    #[test]
    fn a_ready_device_code_poll_persists_the_session_and_never_returns_it() {
        let server = test_http::serve_sequence(&[
            (
                200,
                &json!({
                    "code": 0,
                    "data": {
                        "accessToken": "access-1",
                        "refreshToken": "refresh-1",
                        "domain": "www.codebuddy.cn"
                    }
                })
                .to_string(),
            ),
            (
                200,
                &json!({
                    "code": 0,
                    "data": {"uid": "uid-1", "nickname": "Ling"}
                })
                .to_string(),
            ),
        ]);
        let provider =
            WorkBuddyProvider::for_test_with_session(None, None).with_test_base_url(&server);
        let login_id = provider.login.register_for_test("st-1");

        let poll = provider.poll_device_code_login(&login_id);

        assert!(poll.done);
        assert_eq!(poll.error, None);
        let stored = provider
            .sessions
            .load()
            .unwrap()
            .expect("a ready poll must persist the session");
        assert_eq!(stored.access_token, "access-1");
        assert_eq!(stored.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(stored.uid.as_deref(), Some("uid-1"));
    }

    #[test]
    fn a_session_that_cannot_be_persisted_is_reported_as_an_error() {
        let server = test_http::serve_sequence(&[
            (
                200,
                &json!({
                    "code": 0,
                    "data": {"accessToken": "access-1", "domain": "www.codebuddy.cn"}
                })
                .to_string(),
            ),
            (
                200,
                &json!({"code": 0, "data": {"uid": "uid-1"}}).to_string(),
            ),
        ]);
        let mut provider =
            WorkBuddyProvider::for_test_with_session(None, None).with_test_base_url(&server);
        provider.sessions = failing_session_store();
        let login_id = provider.login.register_for_test("st-1");

        let poll = provider.poll_device_code_login(&login_id);

        assert!(poll.done);
        assert!(
            poll.error.is_some(),
            "a persistence failure must never be swallowed"
        );
    }

    #[test]
    fn cache_identity_prefers_the_session_and_falls_back_to_the_login_file() {
        let session_only = WorkBuddyProvider::for_test_with_session(Some("access-session"), None);
        let file_only = WorkBuddyProvider::for_test_with_session(None, Some("access-file"));
        let both =
            WorkBuddyProvider::for_test_with_session(Some("access-session"), Some("access-file"));

        assert_ne!(session_only.cache_identity(), file_only.cache_identity());
        assert_eq!(both.cache_identity(), session_only.cache_identity());
    }
}
