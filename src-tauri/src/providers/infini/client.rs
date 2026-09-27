use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::InfiniError;

/// Coding Plan request accounting. This is a separate service from the GenStudio
/// LLM API, so a GenStudio key (`sk-…`) is rejected by this endpoint — Coding Plan
/// keys are issued as `sk-cp-…`.
pub const USAGE_URL: &str = "https://cloud.infini-ai.com/maas/coding/usage";

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct InfiniClient {
    timeout: Duration,
    url: String,
}

impl InfiniClient {
    pub fn new() -> Result<Self, InfiniError> {
        Self::with_endpoint(USAGE_URL, Duration::from_secs(15))
    }

    fn with_endpoint(url: &str, timeout: Duration) -> Result<Self, InfiniError> {
        Ok(Self {
            timeout,
            url: url.to_owned(),
        })
    }

    fn http_client(&self, context: &ProviderRequestContext) -> Result<Arc<Client>, InfiniError> {
        context
            .http_clients
            .client("infini", "default", context.proxy_url.as_ref(), |builder| {
                builder
                    .connect_timeout(Duration::from_secs(8))
                    .timeout(self.timeout)
                    .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
            })
            .map_err(|_| InfiniError::ConnectionFailed)
    }

    pub fn fetch(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<EndpointResponse, InfiniError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(&self.url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "infini coding usage request failed (transport)");
                InfiniError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "infini coding usage HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response.text().map_err(|_| InfiniError::InvalidResponse)?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(EndpointResponse { status, body })
    }
}

#[cfg(test)]
impl InfiniClient {
    pub fn for_test(url: &str, timeout: Duration) -> Self {
        Self::with_endpoint(url, timeout).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use reqwest::Url;

    use super::InfiniClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    #[test]
    fn provider_proxy_api_key_route_infini() {
        let (base_url, request) =
            test_http::serve_once_capturing_request(200, r#"{"marker":"proxy"}"#);
        let proxy_url = base_url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let client = InfiniClient::for_test(&format!("{base_url}/usage"), Duration::from_secs(1));
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };

        let response = client.fetch(&context, "provider-api-key").unwrap();

        assert_eq!(response.body["marker"], "proxy");
        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("authorization: bearer provider-api-key"));
    }
}
