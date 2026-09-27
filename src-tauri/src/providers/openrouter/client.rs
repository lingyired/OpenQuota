use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::OpenRouterError;

const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";
const KEY_URL: &str = "https://openrouter.ai/api/v1/key";

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct OpenRouterClient {
    timeout: Duration,
    credits_url: String,
    key_url: String,
}

impl OpenRouterClient {
    pub fn new() -> Result<Self, OpenRouterError> {
        Self::with_endpoints(CREDITS_URL, KEY_URL, Duration::from_secs(15))
    }

    fn with_endpoints(
        credits_url: &str,
        key_url: &str,
        timeout: Duration,
    ) -> Result<Self, OpenRouterError> {
        Ok(Self {
            timeout,
            credits_url: credits_url.to_owned(),
            key_url: key_url.to_owned(),
        })
    }

    fn http_client(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<Arc<Client>, OpenRouterError> {
        context
            .http_clients
            .client(
                "openrouter",
                "default",
                context.proxy_url.as_ref(),
                |builder| {
                    builder
                        .connect_timeout(Duration::from_secs(8))
                        .timeout(self.timeout)
                        .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
                },
            )
            .map_err(|_| OpenRouterError::ConnectionFailed)
    }

    pub fn fetch_credits(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<EndpointResponse, OpenRouterError> {
        self.fetch(context, &self.credits_url, api_key, "credits")
    }

    pub fn fetch_key(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<EndpointResponse, OpenRouterError> {
        self.fetch(context, &self.key_url, api_key, "key")
    }

    fn fetch(
        &self,
        context: &ProviderRequestContext,
        url: &str,
        api_key: &str,
        endpoint: &str,
    ) -> Result<EndpointResponse, OpenRouterError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "openrouter {endpoint} request failed (transport)");
                OpenRouterError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "openrouter {endpoint} HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response
            .text()
            .map_err(|_| OpenRouterError::InvalidResponse)?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(EndpointResponse { status, body })
    }
}

#[cfg(test)]
impl OpenRouterClient {
    pub fn for_test(credits_url: &str, key_url: &str, timeout: Duration) -> Self {
        Self::with_endpoints(credits_url, key_url, timeout).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use reqwest::Url;

    use super::OpenRouterClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    #[test]
    fn provider_proxy_remaining_route_openrouter() {
        let (url, request) = test_http::serve_once_capturing_request(200, r#"{"data":{}}"#);
        let proxy_url = url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };
        let client = OpenRouterClient::for_test(&url, &url, Duration::from_secs(1));

        client.fetch_credits(&context, "provider-api-key").unwrap();

        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("authorization: bearer provider-api-key"));
        assert!(request.contains("accept: application/json"));
        assert!(request.contains(concat!("user-agent: quota01/", env!("CARGO_PKG_VERSION"))));
    }
}
