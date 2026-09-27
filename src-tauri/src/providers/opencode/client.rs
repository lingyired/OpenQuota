use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::OpenCodeError;

const GO_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

#[derive(Debug)]
pub(super) struct UsageResponse {
    pub(super) status: StatusCode,
    pub(super) body: Value,
}

pub(super) struct OpenCodeClient {
    timeout: Duration,
    usage_url: String,
}

impl OpenCodeClient {
    pub(super) fn new() -> Result<Self, OpenCodeError> {
        Self::with_endpoint(GO_USAGE_URL, Duration::from_secs(15))
    }

    fn with_endpoint(usage_url: &str, timeout: Duration) -> Result<Self, OpenCodeError> {
        Ok(Self {
            timeout,
            usage_url: usage_url.into(),
        })
    }

    fn http_client(&self, context: &ProviderRequestContext) -> Result<Arc<Client>, OpenCodeError> {
        context
            .http_clients
            .client(
                "opencode",
                "default",
                context.proxy_url.as_ref(),
                |builder| {
                    builder
                        .connect_timeout(Duration::from_secs(8))
                        .timeout(self.timeout)
                        .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
                },
            )
            .map_err(|_| OpenCodeError::ConnectionFailed)
    }

    pub(super) fn fetch_go_usage(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<UsageResponse, OpenCodeError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(&self.usage_url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "opencode go usage request failed (transport)");
                OpenCodeError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "opencode go usage HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let body = response
            .text()
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        Ok(UsageResponse { status, body })
    }
}

#[cfg(test)]
impl OpenCodeClient {
    pub(super) fn for_test(url: &str, timeout: Duration) -> Self {
        Self::with_endpoint(url, timeout).expect("test OpenCode endpoint should be valid")
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use reqwest::Url;

    use super::OpenCodeClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    #[test]
    fn provider_proxy_remaining_route_opencode() {
        let (url, request) = test_http::serve_once_capturing_request(200, r#"{"usage":{}}"#);
        let proxy_url = url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };
        let client = OpenCodeClient::for_test(&url, Duration::from_secs(1));

        client.fetch_go_usage(&context, "provider-api-key").unwrap();

        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("authorization: bearer provider-api-key"));
        assert!(request.contains("accept: application/json"));
        assert!(request.contains(concat!("user-agent: quota01/", env!("CARGO_PKG_VERSION"))));
    }
}
