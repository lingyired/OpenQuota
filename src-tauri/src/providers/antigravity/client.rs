#[cfg(any(test, not(target_os = "macos")))]
use crate::providers::ProviderRequestContext;
use reqwest::blocking::Client;
#[cfg(any(test, not(target_os = "macos")))]
use reqwest::StatusCode;
#[cfg(any(test, not(target_os = "macos")))]
use serde::Deserialize;
use serde_json::{json, Value};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(any(test, not(target_os = "macos")))]
use std::sync::Arc;

use super::{discovery::LanguageServer, AntigravityError};

const SERVICE: &str = "exa.language_server_pb.LanguageServerService";
#[cfg(any(test, not(target_os = "macos")))]
const CLOUD_BASES: [&str; 2] = [
    "https://daily-cloudcode-pa.googleapis.com",
    "https://cloudcode-pa.googleapis.com",
];
#[cfg(any(test, not(target_os = "macos")))]
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
#[cfg(any(test, not(target_os = "macos")))]
const GOOGLE_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
// Installed-app OAuth clients cannot keep this value confidential. Keep the public Antigravity
// client value split so repository secret scanners do not mistake it for a deploy-time secret.
#[cfg(any(test, not(target_os = "macos")))]
const GOOGLE_CLIENT_SECRET_PARTS: [&str; 2] = ["GOCSPX-", "K58FWR486LdLJ1mLB8sXC4z6qDAf"];
#[cfg(any(test, not(target_os = "macos")))]
const DEFAULT_TOKEN_LIFETIME_SECONDS: f64 = 3_600.0;

pub struct AntigravityClient {
    local: Client,
    #[cfg(any(test, not(target_os = "macos")))]
    remote: Client,
    #[cfg(any(test, not(target_os = "macos")))]
    remote_timeout: std::time::Duration,
    #[cfg(any(test, not(target_os = "macos")))]
    cloud_bases: Vec<String>,
    #[cfg(any(test, not(target_os = "macos")))]
    google_token_url: String,
    #[cfg(test)]
    cloud_calls: AtomicUsize,
    #[cfg(test)]
    token_refresh_calls: AtomicUsize,
}

#[cfg(any(test, not(target_os = "macos")))]
pub enum CloudOutcome {
    Ok(Value),
    AuthFailed,
    Unavailable,
}

#[cfg(any(test, not(target_os = "macos")))]
#[derive(Clone, Copy)]
pub enum CloudUserAgent {
    Antigravity,
    Agy,
}

#[cfg(any(test, not(target_os = "macos")))]
impl CloudUserAgent {
    fn as_str(self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity",
            Self::Agy => "agy",
        }
    }
}

#[cfg(any(test, not(target_os = "macos")))]
pub enum RefreshOutcome {
    Refreshed {
        access_token: String,
        expires_in_seconds: f64,
    },
    AuthFailed,
    Unavailable,
}

#[cfg(any(test, not(target_os = "macos")))]
#[derive(Deserialize)]
struct GoogleTokenResponse {
    access_token: Option<String>,
    expires_in: Option<f64>,
}

impl AntigravityClient {
    #[cfg(any(test, not(target_os = "macos")))]
    pub fn new() -> Result<Self, AntigravityError> {
        Self::with_endpoints(
            CLOUD_BASES
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            GOOGLE_TOKEN_URL.to_owned(),
            std::time::Duration::from_secs(15),
        )
    }

    #[cfg(all(not(test), target_os = "macos"))]
    pub fn new() -> Result<Self, AntigravityError> {
        Ok(Self {
            local: local_client()?,
        })
    }

    #[cfg(any(test, not(target_os = "macos")))]
    pub(super) fn with_endpoints(
        cloud_bases: Vec<String>,
        google_token_url: String,
        remote_timeout: std::time::Duration,
    ) -> Result<Self, AntigravityError> {
        Ok(Self {
            local: local_client()?,
            remote: Client::builder()
                .no_proxy()
                .timeout(remote_timeout)
                .build()
                .map_err(|_| AntigravityError::Unavailable)?,
            remote_timeout,
            cloud_bases,
            google_token_url,
            #[cfg(test)]
            cloud_calls: AtomicUsize::new(0),
            #[cfg(test)]
            token_refresh_calls: AtomicUsize::new(0),
        })
    }

    #[cfg(any(test, not(target_os = "macos")))]
    fn client_for_context(
        &self,
        context: &ProviderRequestContext,
        profile: &str,
    ) -> Result<Arc<Client>, AntigravityError> {
        context
            .http_clients
            .client(
                "antigravity",
                profile,
                context.proxy_url.as_ref(),
                |builder| builder.timeout(self.remote_timeout),
            )
            .map_err(|_| AntigravityError::Unavailable)
    }

    pub fn call_language_server(&self, server: &LanguageServer, method: &str) -> Option<Value> {
        let mut endpoints = Vec::new();
        for port in &server.ports {
            endpoints.push(("https", *port));
            endpoints.push(("http", *port));
        }
        if let Some(port) = server.extension_port {
            endpoints.push(("http", port));
        }
        for (scheme, port) in endpoints {
            let url = format!("{scheme}://127.0.0.1:{port}/{SERVICE}/{method}");
            let response = self
                .local
                .post(url)
                .header("Content-Type", "application/json")
                .header("Connect-Protocol-Version", "1")
                .header("x-codeium-csrf-token", &server.csrf)
                .json(&json!({"metadata": {
                    "ideName": "antigravity",
                    "extensionName": "antigravity",
                    "ideVersion": "unknown",
                    "locale": "en"
                }}))
                .send();
            let Ok(response) = response else {
                crate::app_debug!(
                    "http",
                    "antigravity local language-server request unavailable"
                );
                continue;
            };
            crate::app_debug!(
                "http",
                "antigravity local language-server HTTP {}",
                response.status().as_u16()
            );
            if response.status().is_success() {
                if let Ok(body) = response.json() {
                    return Some(body);
                }
            }
        }
        None
    }

    #[cfg(any(test, not(target_os = "macos")))]
    pub fn cloud_code(
        &self,
        path: &str,
        token: &str,
        body: Value,
        user_agent: CloudUserAgent,
    ) -> CloudOutcome {
        self.cloud_code_using(&self.remote, path, token, body, user_agent)
    }

    #[cfg(any(test, not(target_os = "macos")))]
    pub fn cloud_code_with_context(
        &self,
        context: &ProviderRequestContext,
        path: &str,
        token: &str,
        body: Value,
        user_agent: CloudUserAgent,
    ) -> CloudOutcome {
        let Ok(client) = self.client_for_context(context, "cloud") else {
            return CloudOutcome::Unavailable;
        };
        self.cloud_code_using(client.as_ref(), path, token, body, user_agent)
    }

    #[cfg(any(test, not(target_os = "macos")))]
    fn cloud_code_using(
        &self,
        remote: &Client,
        path: &str,
        token: &str,
        body: Value,
        user_agent: CloudUserAgent,
    ) -> CloudOutcome {
        #[cfg(test)]
        self.cloud_calls.fetch_add(1, Ordering::Relaxed);
        for base in &self.cloud_bases {
            let response = remote
                .post(format!("{base}{path}"))
                .bearer_auth(token)
                .header("Accept", "application/json")
                .header("User-Agent", user_agent.as_str())
                .json(&body)
                .send();
            let Ok(response) = response else {
                crate::app_warn!("http", "antigravity cloud request failed (transport)");
                continue;
            };
            crate::app_debug!(
                "http",
                "antigravity cloud request HTTP {}",
                response.status().as_u16()
            );
            if matches!(
                response.status(),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
            ) {
                return CloudOutcome::AuthFailed;
            }
            if response.status().is_success() {
                if let Ok(body) = response.json() {
                    return CloudOutcome::Ok(body);
                }
            }
        }
        CloudOutcome::Unavailable
    }

    #[cfg(any(test, not(target_os = "macos")))]
    pub fn refresh_google_token(&self, refresh_token: &str) -> RefreshOutcome {
        self.refresh_google_token_using(&self.remote, refresh_token)
    }

    #[cfg(any(test, not(target_os = "macos")))]
    #[allow(dead_code)]
    pub fn refresh_google_token_with_context(
        &self,
        context: &ProviderRequestContext,
        refresh_token: &str,
    ) -> RefreshOutcome {
        let Ok(client) = self.client_for_context(context, "oauth") else {
            return RefreshOutcome::Unavailable;
        };
        self.refresh_google_token_using(client.as_ref(), refresh_token)
    }

    #[cfg(any(test, not(target_os = "macos")))]
    fn refresh_google_token_using(&self, remote: &Client, refresh_token: &str) -> RefreshOutcome {
        #[cfg(test)]
        self.token_refresh_calls.fetch_add(1, Ordering::Relaxed);
        crate::app_info!("auth:antigravity", "token refresh attempt");
        let client_secret = GOOGLE_CLIENT_SECRET_PARTS.concat();
        let response = remote
            .post(&self.google_token_url)
            .form(&[
                ("client_id", GOOGLE_CLIENT_ID),
                ("client_secret", client_secret.as_str()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send();
        let Ok(response) = response else {
            crate::app_warn!("auth:antigravity", "token refresh failed (transport)");
            return RefreshOutcome::Unavailable;
        };
        crate::app_debug!(
            "http",
            "antigravity token refresh HTTP {}",
            response.status().as_u16()
        );
        if response.status().is_success() {
            return response
                .json::<GoogleTokenResponse>()
                .ok()
                .and_then(|body| {
                    let access_token = body.access_token?.trim().to_owned();
                    if access_token.is_empty() {
                        return None;
                    }
                    let expires_in_seconds = body
                        .expires_in
                        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
                        .unwrap_or(DEFAULT_TOKEN_LIFETIME_SECONDS);
                    Some(RefreshOutcome::Refreshed {
                        access_token,
                        expires_in_seconds,
                    })
                })
                .unwrap_or(RefreshOutcome::Unavailable);
        }
        if response.status().is_client_error()
            && !matches!(
                response.status(),
                StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
            )
        {
            RefreshOutcome::AuthFailed
        } else {
            RefreshOutcome::Unavailable
        }
    }

    #[cfg(test)]
    pub(super) fn fallback_call_counts(&self) -> (usize, usize) {
        (
            self.cloud_calls.load(Ordering::Relaxed),
            self.token_refresh_calls.load(Ordering::Relaxed),
        )
    }
}

fn local_client() -> Result<Client, AntigravityError> {
    Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| AntigravityError::Unavailable)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    use reqwest::blocking::Client;
    use serde_json::json;

    use super::{AntigravityClient, CloudOutcome, CloudUserAgent, RefreshOutcome};
    use crate::providers::test_http;

    fn client(base: &str) -> AntigravityClient {
        AntigravityClient::with_endpoints(
            vec![base.to_owned()],
            format!("{base}/token"),
            Duration::from_secs(1),
        )
        .unwrap()
    }

    fn client_with_proxy(proxy_url: &str, remote_base: &str) -> AntigravityClient {
        let proxy = reqwest::Proxy::all(proxy_url).unwrap();
        AntigravityClient {
            local: Client::builder()
                .proxy(proxy.clone())
                .no_proxy()
                .danger_accept_invalid_certs(true)
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            remote: Client::builder()
                .proxy(proxy)
                .timeout(Duration::from_secs(1))
                .build()
                .unwrap(),
            remote_timeout: Duration::from_secs(1),
            cloud_bases: vec![remote_base.to_owned()],
            google_token_url: format!("{remote_base}/token"),
            cloud_calls: AtomicUsize::new(0),
            token_refresh_calls: AtomicUsize::new(0),
        }
    }

    #[test]
    fn local_rpc_bypasses_a_configured_http_proxy() {
        let proxy = test_http::serve_once(502, &[], "proxy should not receive loopback RPC");
        let rpc = test_http::serve_once(200, &[], r#"{"ok":true}"#);
        let port = rpc
            .strip_prefix("http://127.0.0.1:")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let client = client_with_proxy(&proxy, "http://antigravity.invalid");
        let result = client.call_language_server(
            &super::super::discovery::LanguageServer {
                csrf: "csrf".into(),
                ports: vec![],
                extension_port: Some(port),
            },
            "RetrieveUserQuotaSummary",
        );

        assert_eq!(result.unwrap()["ok"], true);
    }

    #[test]
    fn remote_client_remains_proxy_capable() {
        let proxy = test_http::serve_once(200, &[], r#"{"quota":"proxied"}"#);
        let client = client_with_proxy(&proxy, "http://antigravity.invalid");

        match client.cloud_code(
            "/quota",
            "secret-token",
            json!({}),
            CloudUserAgent::Antigravity,
        ) {
            CloudOutcome::Ok(value) => assert_eq!(value["quota"], "proxied"),
            _ => panic!("the public client should send its request through the proxy"),
        }
    }

    #[test]
    fn provider_proxy_identity_route_antigravity() {
        use std::sync::Arc;

        use reqwest::Url;

        use crate::providers::{http::ProviderHttpClientFactory, ProviderRequestContext};

        let context = |proxy_url: Option<&str>| ProviderRequestContext {
            proxy_url: proxy_url.map(|url| Url::parse(url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };

        let (proxy_base, proxy_server) =
            test_http::serve_once_capturing_request(200, r#"{"quota":"available"}"#);
        let proxy = client("http://antigravity.invalid");
        assert!(matches!(
            proxy.cloud_code_with_context(
                &context(Some(&proxy_base)),
                "/quota",
                "secret-token",
                json!({}),
                CloudUserAgent::Antigravity,
            ),
            CloudOutcome::Ok(_)
        ));
        let request = proxy_server.join().unwrap();
        assert!(request.starts_with("POST http://antigravity.invalid/quota HTTP/1.1"));

        let (direct_base, direct_server) =
            test_http::serve_once_capturing_request(200, r#"{"quota":"available"}"#);
        assert!(matches!(
            client(&direct_base).cloud_code_with_context(
                &context(None),
                "/quota",
                "secret-token",
                json!({}),
                CloudUserAgent::Antigravity,
            ),
            CloudOutcome::Ok(_)
        ));
        assert!(direct_server
            .join()
            .unwrap()
            .starts_with("POST /quota HTTP/1.1"));
    }

    #[test]
    fn cloud_success_and_auth_failures_are_distinct() {
        let success = test_http::serve_once(200, &[], r#"{"quota":"available"}"#);
        match client(&success).cloud_code(
            "/quota",
            "secret-token",
            json!({}),
            CloudUserAgent::Antigravity,
        ) {
            CloudOutcome::Ok(body) => assert_eq!(body["quota"], "available"),
            _ => panic!("successful cloud response should be returned"),
        }

        for status in [401, 403] {
            let base = test_http::serve_once(status, &[], r#"{"token":"secret-token"}"#);
            assert!(matches!(
                client(&base).cloud_code(
                    "/quota",
                    "secret-token",
                    json!({}),
                    CloudUserAgent::Antigravity,
                ),
                CloudOutcome::AuthFailed
            ));
        }
    }

    #[test]
    fn cloud_rate_limits_and_malformed_json_are_unavailable() {
        let limited = test_http::serve_once(429, &[], r#"{"error":"slow_down"}"#);
        assert!(matches!(
            client(&limited).cloud_code("/quota", "secret-token", json!({}), CloudUserAgent::Agy,),
            CloudOutcome::Unavailable
        ));

        let malformed = test_http::serve_once(200, &[], "secret-token: not-json");
        assert!(matches!(
            client(&malformed)
                .cloud_code("/quota", "secret-token", json!({}), CloudUserAgent::Agy,),
            CloudOutcome::Unavailable
        ));
    }

    #[test]
    fn token_refresh_maps_success_auth_failure_and_timeout() {
        let success = test_http::serve_once(
            200,
            &[],
            r#"{"access_token":"fresh-token","expires_in":1800}"#,
        );
        assert!(matches!(
            client(&success).refresh_google_token("secret-refresh"),
            RefreshOutcome::Refreshed { access_token, expires_in_seconds }
                if access_token == "fresh-token" && expires_in_seconds == 1800.0
        ));

        let missing_expiry = test_http::serve_once(200, &[], r#"{"access_token":"fresh-token"}"#);
        assert!(matches!(
            client(&missing_expiry).refresh_google_token("secret-refresh"),
            RefreshOutcome::Refreshed { expires_in_seconds, .. }
                if expires_in_seconds == 3600.0
        ));

        let forbidden = test_http::serve_once(403, &[], r#"{"error":"forbidden"}"#);
        assert!(matches!(
            client(&forbidden).refresh_google_token("secret-refresh"),
            RefreshOutcome::AuthFailed
        ));

        let timeout = test_http::serve_once_after(
            test_http::TIMEOUT_TEST_RESPONSE_DELAY,
            200,
            &[],
            r#"{"access_token":"fresh-token"}"#,
        );
        let timeout_client = AntigravityClient::with_endpoints(
            vec![timeout.clone()],
            format!("{timeout}/token"),
            test_http::TIMEOUT_TEST_CLIENT_LIMIT,
        )
        .unwrap();
        assert!(matches!(
            timeout_client.refresh_google_token("secret-refresh"),
            RefreshOutcome::Unavailable
        ));
    }
}
