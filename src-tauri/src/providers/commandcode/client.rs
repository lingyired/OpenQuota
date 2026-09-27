use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::CommandCodeError;

const API_BASE: &str = "https://api.commandcode.ai";
const CREDITS_PATH: &str = "/alpha/billing/credits";
const SUBSCRIPTIONS_PATH: &str = "/alpha/billing/subscriptions";

#[derive(Debug, Default)]
pub(super) struct BillingResponse {
    pub(super) status: StatusCode,
    pub(super) body: Value,
}

pub(super) struct CommandCodeClient {
    timeout: Duration,
    api_base: String,
}

impl CommandCodeClient {
    pub(super) fn new() -> Result<Self, CommandCodeError> {
        Self::with_base_url(None)
    }

    fn with_base_url(test_base_url: Option<String>) -> Result<Self, CommandCodeError> {
        Ok(Self {
            timeout: Duration::from_secs(15),
            api_base: test_base_url
                .unwrap_or_else(|| API_BASE.to_owned())
                .trim_end_matches('/')
                .to_owned(),
        })
    }

    fn http_client(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<Arc<Client>, CommandCodeError> {
        context
            .http_clients
            .client(
                "commandcode",
                "default",
                context.proxy_url.as_ref(),
                |builder| {
                    builder
                        .connect_timeout(Duration::from_secs(8))
                        .timeout(self.timeout)
                        .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
                },
            )
            .map_err(|_| CommandCodeError::ConnectionFailed)
    }

    pub(super) fn fetch_credits(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<BillingResponse, CommandCodeError> {
        self.fetch(context, CREDITS_PATH, api_key, "credits")
    }

    /// The subscription carries the plan name (Go / GOAT / Pro / Max / Ultra)
    /// and the monthly reset date, but the endpoint is secondary: a failure
    /// there degrades to a generic report instead of failing the refresh.
    pub(super) fn fetch_subscription(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<BillingResponse, CommandCodeError> {
        self.fetch(context, SUBSCRIPTIONS_PATH, api_key, "subscription")
    }

    fn fetch(
        &self,
        context: &ProviderRequestContext,
        path: &str,
        api_key: &str,
        endpoint: &str,
    ) -> Result<BillingResponse, CommandCodeError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(format!("{}{path}", self.api_base))
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "commandcode {endpoint} request failed (transport)");
                CommandCodeError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "commandcode {endpoint} HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let body = response
            .text()
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        Ok(BillingResponse { status, body })
    }
}

#[cfg(test)]
impl CommandCodeClient {
    pub(super) fn for_test(base_url: &str) -> Self {
        Self::with_base_url(Some(base_url.to_owned()))
            .expect("test CommandCode endpoint should be valid")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::Url;

    use super::CommandCodeClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    #[test]
    fn provider_proxy_remaining_route_commandcode() {
        let (url, request) = test_http::serve_once_capturing_request(200, r#"{"credits":{}}"#);
        let proxy_url = url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };
        let client = CommandCodeClient::for_test(&url);

        let response = client.fetch_credits(&context, "provider-api-key").unwrap();

        assert_eq!(response.status.as_u16(), 200);
        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("authorization: bearer provider-api-key"));
    }
}
