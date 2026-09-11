use std::time::Duration;

use chrono::{DateTime, Local};
use reqwest::{blocking::Client, StatusCode};
use serde_json::{json, Value};
use thiserror::Error;

use super::auth::WorkBuddyAuth;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36";
const PAID_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_002_AkiJS3ZHF5",
    "TCACA_code_023_4xbGhMrE6q",
    "TCACA_code_026_BaESVICNoi",
    "TCACA_code_027_0FCGVA6vSa",
    "TCACA_code_009_0XmEQc2xOf",
    "TCACA_code_038_OhvqZtiPKr",
];
const FREE_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_008_cfWoLwvjU4",
    "TCACA_code_007_nzdH5h4Nl0",
    "TCACA_code_028_NtpWi0jzXs",
    "TCACA_code_029_6wCGEWquYy",
    "TCACA_code_030_BjSt89qTvr",
];

#[derive(Debug, Error)]
pub enum WorkBuddyClientError {
    #[error("Could not reach WorkBuddy.")]
    Connection,
    #[error("WorkBuddy returned an invalid response.")]
    InvalidResponse,
    #[error("WorkBuddy credentials could not be refreshed.")]
    RefreshFailed,
}

#[derive(Debug, Clone)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

impl EndpointResponse {
    pub fn code(&self) -> Option<i64> {
        self.body
            .get("code")
            .or_else(|| self.body.get("data").and_then(|data| data.get("code")))
            .and_then(number_i64)
    }

    pub fn message(&self) -> Option<&str> {
        ["message", "msg", "error"]
            .iter()
            .find_map(|key| self.body.get(*key).and_then(Value::as_str))
            .or_else(|| {
                self.body.get("data").and_then(|data| {
                    ["message", "msg", "error"]
                        .iter()
                        .find_map(|key| data.get(*key).and_then(Value::as_str))
                })
            })
    }

    pub fn is_success(&self) -> bool {
        if !self.status.is_success() || self.has_explicit_failure() {
            return false;
        }
        match self.code() {
            Some(code) => code == 0 || code == 200,
            None => self.body.get("data").is_some_and(|data| !data.is_null()),
        }
    }

    fn has_explicit_failure(&self) -> bool {
        [self.body.get("ok"), self.body.get("success")]
            .into_iter()
            .flatten()
            .any(|value| value.as_bool() == Some(false))
            || self.body.get("data").is_some_and(|data| {
                [data.get("ok"), data.get("success")]
                    .into_iter()
                    .flatten()
                    .any(|value| value.as_bool() == Some(false))
            })
    }

    pub fn is_http_forbidden(&self) -> bool {
        self.status == StatusCode::FORBIDDEN
    }

    pub fn is_unauthorized(&self) -> bool {
        self.status == StatusCode::UNAUTHORIZED
            || self.code().is_some_and(|code| code == 401 || code == 403)
            || self.message().is_some_and(|message| {
                let lower = message.to_ascii_lowercase();
                lower.contains("unauthorized")
                    || lower.contains("401")
                    || lower.contains("expired")
                    || lower.contains("invalid")
                    || lower.contains("失效")
                    || lower.contains("过期")
                    || lower.contains("token")
                    || message.contains("登录")
            })
    }

    pub fn is_waf(&self) -> bool {
        self.code() == Some(10085)
    }

    pub fn is_retryable_transport(&self) -> bool {
        self.code() == Some(-1)
            && self
                .message()
                .is_some_and(|message| !message.trim().is_empty())
    }
}

pub struct WorkBuddyClient {
    client: Client,
    test_base_url: Option<String>,
}

impl WorkBuddyClient {
    pub fn new() -> Result<Self, WorkBuddyClientError> {
        Self::with_base_url(None)
    }

    fn with_base_url(test_base_url: Option<String>) -> Result<Self, WorkBuddyClientError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|_| WorkBuddyClientError::Connection)?;
        Ok(Self {
            client,
            test_base_url,
        })
    }

    fn base_url(&self, auth: &WorkBuddyAuth, usage: bool) -> String {
        if let Some(base) = &self.test_base_url {
            return base.trim_end_matches('/').to_owned();
        }
        if usage {
            auth.usage_base_url().to_owned()
        } else {
            auth.request_base_url().to_owned()
        }
    }

    pub fn fetch_resource_summary(
        &self,
        auth: &WorkBuddyAuth,
        now: DateTime<Local>,
    ) -> Result<EndpointResponse, WorkBuddyClientError> {
        self.post_with_retry(
            auth,
            false,
            "/billing/meter/get-user-resource-summary",
            json!({}),
            "summary",
            now,
        )
    }

    pub fn fetch_paid_packages(
        &self,
        auth: &WorkBuddyAuth,
        now: DateTime<Local>,
    ) -> Result<EndpointResponse, WorkBuddyClientError> {
        self.post_with_retry(
            auth,
            false,
            "/billing/meter/get-user-resource-paid-packages",
            paid_packages_payload(),
            "paid-packages",
            now,
        )
    }

    pub fn fetch_free_packages(
        &self,
        auth: &WorkBuddyAuth,
        now: DateTime<Local>,
    ) -> Result<EndpointResponse, WorkBuddyClientError> {
        let date = now.format("%Y-%m-%d").to_string();
        self.post_with_retry(
            auth,
            false,
            "/billing/meter/get-user-resource-free-packages",
            free_packages_payload(&date),
            "free-packages",
            now,
        )
    }

    pub fn fetch_usage_page(
        &self,
        auth: &WorkBuddyAuth,
        start: DateTime<Local>,
        end: DateTime<Local>,
        page: u32,
    ) -> Result<EndpointResponse, WorkBuddyClientError> {
        self.post_with_retry(
            auth,
            true,
            "/billing/meter/get-user-request-usage",
            json!({
                "startTime": start.format("%Y-%m-%d %H:%M:%S").to_string(),
                "endTime": end.format("%Y-%m-%d %H:%M:%S").to_string(),
                "pageNum": page,
                "pageSize": 3000,
            }),
            "usage",
            end,
        )
    }

    pub fn refresh_token(
        &self,
        auth: &WorkBuddyAuth,
    ) -> Result<(String, Option<String>), WorkBuddyClientError> {
        let url = self.url(auth, false, "/v2/plugin/auth/token/refresh");
        let request = self.request(auth, &url, json!({}));
        let response = request
            .header(
                "X-Refresh-Token",
                auth.refresh_token.as_deref().unwrap_or_default(),
            )
            .send()
            .map_err(|_| WorkBuddyClientError::Connection)?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .map_err(|_| WorkBuddyClientError::InvalidResponse)?;
        let endpoint = EndpointResponse { status, body };
        if !endpoint.is_success() {
            return Err(WorkBuddyClientError::RefreshFailed);
        }
        let data = endpoint.body.get("data").unwrap_or(&endpoint.body);
        let access = data
            .get("accessToken")
            .or_else(|| data.get("access_token"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or(WorkBuddyClientError::RefreshFailed)?
            .to_owned();
        let refresh = data
            .get("refreshToken")
            .or_else(|| data.get("refresh_token"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok((access, refresh))
    }

    fn post_with_retry(
        &self,
        auth: &WorkBuddyAuth,
        usage: bool,
        path: &str,
        payload: Value,
        label: &str,
        _now: impl Into<DateTime<Local>>,
    ) -> Result<EndpointResponse, WorkBuddyClientError> {
        let response = self.post_once(auth, usage, path, &payload, label)?;
        if response.is_retryable_transport() {
            return self.post_once(auth, usage, path, &payload, label);
        }
        Ok(response)
    }

    fn post_once(
        &self,
        auth: &WorkBuddyAuth,
        usage: bool,
        path: &str,
        payload: &Value,
        label: &str,
    ) -> Result<EndpointResponse, WorkBuddyClientError> {
        let url = self.url(auth, usage, path);
        let started = std::time::Instant::now();
        let response = self
            .request(auth, &url, payload.clone())
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "workbuddy {label} request failed (transport)");
                WorkBuddyClientError::Connection
            })?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .map_err(|_| WorkBuddyClientError::InvalidResponse)?;
        crate::app_debug!(
            "http",
            "workbuddy {label} HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        Ok(EndpointResponse { status, body })
    }

    fn request(
        &self,
        auth: &WorkBuddyAuth,
        url: &str,
        payload: Value,
    ) -> reqwest::blocking::RequestBuilder {
        let host = url
            .split("//")
            .nth(1)
            .and_then(|part| part.split('/').next())
            .unwrap_or("www.codebuddy.cn");
        let mut request = self
            .client
            .post(url)
            .json(&payload)
            .header("Accept", "application/json, text/plain, */*")
            .header(
                "Authorization",
                format!("{} {}", auth.token_type, auth.access_token),
            )
            .header("X-Domain", &auth.domain)
            .header("X-Client-Platform", "web")
            .header("Origin", format!("https://{host}"))
            .header("Referer", format!("https://{host}/profile/plans-usage"));
        if let Some(uid) = &auth.uid {
            request = request.header("X-User-Id", uid);
        }
        if let Some(enterprise_id) = &auth.enterprise_id {
            request = request
                .header("X-Enterprise-Id", enterprise_id)
                .header("X-Tenant-Id", enterprise_id);
        }
        request
    }

    fn url(&self, auth: &WorkBuddyAuth, usage: bool, path: &str) -> String {
        format!("{}{path}", self.base_url(auth, usage))
    }
}

fn paid_packages_payload() -> Value {
    json!({
        "PageNumber": 1,
        "PageSize": 200,
        "Status": [0, 3],
        "PackageCodes": PAID_PACKAGE_CODES,
        "NeedRenewInfo": true,
    })
}

fn free_packages_payload(date: &str) -> Value {
    json!({
        "PageNumber": 1,
        "PageSize": 200,
        "Status": [0, 3],
        "SlicePeriodStartTime": format!("{date} 00:00:00"),
        "SlicePeriodEndTime": format!("{date} 23:59:59"),
        "PackageCodes": FREE_PACKAGE_CODES,
    })
}

fn number_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

#[cfg(test)]
impl WorkBuddyClient {
    pub fn for_test(base_url: &str) -> Self {
        Self::with_base_url(Some(base_url.to_owned())).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::test_http;
    use serde_json::json;

    #[test]
    fn classifies_auth_waf_and_retry_responses() {
        let auth = EndpointResponse {
            status: StatusCode::OK,
            body: json!({"code":401,"message":"token expired"}),
        };
        assert!(auth.is_unauthorized());
        assert!(!auth.is_http_forbidden());
        let http_forbidden = EndpointResponse {
            status: StatusCode::FORBIDDEN,
            body: json!({"message":"permission denied"}),
        };
        assert!(http_forbidden.is_http_forbidden());
        assert!(!http_forbidden.is_success());
        let waf = EndpointResponse {
            status: StatusCode::OK,
            body: json!({"code":10085}),
        };
        assert!(waf.is_waf());
        let retry = EndpointResponse {
            status: StatusCode::OK,
            body: json!({"code":-1,"message":"temporary"}),
        };
        assert!(retry.is_retryable_transport());

        assert!(EndpointResponse {
            status: StatusCode::OK,
            body: json!({"data": {"Packages": []}}),
        }
        .is_success());
        assert!(!EndpointResponse {
            status: StatusCode::OK,
            body: json!({"message": "missing data"}),
        }
        .is_success());
        assert!(!EndpointResponse {
            status: StatusCode::OK,
            body: json!({"data": {"success": false}}),
        }
        .is_success());
    }

    #[test]
    fn package_payloads_follow_the_workbuddy_contract() {
        let paid = paid_packages_payload();
        assert_eq!(paid["PageNumber"], 1);
        assert_eq!(paid["PageSize"], 200);
        assert_eq!(paid["Status"], json!([0, 3]));
        assert_eq!(paid["NeedRenewInfo"], true);
        assert_eq!(paid["PackageCodes"].as_array().unwrap().len(), 6);

        let free = free_packages_payload("2026-09-11");
        assert_eq!(free["SlicePeriodStartTime"], "2026-09-11 00:00:00");
        assert_eq!(free["SlicePeriodEndTime"], "2026-09-11 23:59:59");
        assert_eq!(free["PackageCodes"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn test_server_receives_browser_headers() {
        let server = test_http::serve_once(200, &[], r#"{"code":0,"data":{}}"#);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("auth.info"),
            r#"{"auth":{"accessToken":"token"},"domain":"www.codebuddy.cn","uid":"u"}"#,
        )
        .unwrap();
        let auth = WorkBuddyAuth::load_from_path(&dir.path().join("auth.info")).unwrap();
        let client = WorkBuddyClient::for_test(&server);
        let response = client.fetch_resource_summary(&auth, Local::now()).unwrap();
        assert!(response.is_success());
    }
}
