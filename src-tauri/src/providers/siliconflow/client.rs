use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::{SiliconFlowError, Site};

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct SiliconFlowClient {
    timeout: Duration,
    provider_id: String,
    url: String,
}

impl SiliconFlowClient {
    pub fn new(site: Site) -> Result<Self, SiliconFlowError> {
        Self::with_site_endpoint(site.user_info_url(), Duration::from_secs(15), site.id())
    }

    #[cfg(test)]
    fn with_endpoint(url: &str, timeout: Duration) -> Result<Self, SiliconFlowError> {
        Self::with_site_endpoint(url, timeout, Site::Global.id())
    }

    fn with_site_endpoint(
        url: &str,
        timeout: Duration,
        provider_id: &str,
    ) -> Result<Self, SiliconFlowError> {
        Ok(Self {
            timeout,
            provider_id: provider_id.to_owned(),
            url: url.to_owned(),
        })
    }

    fn http_client(
        &self,
        context: &ProviderRequestContext,
    ) -> Result<Arc<Client>, SiliconFlowError> {
        context
            .http_clients
            .client(
                &self.provider_id,
                "default",
                context.proxy_url.as_ref(),
                |builder| {
                    builder
                        .connect_timeout(Duration::from_secs(8))
                        .timeout(self.timeout)
                        .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
                },
            )
            .map_err(|_| SiliconFlowError::ConnectionFailed)
    }

    pub fn fetch(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<EndpointResponse, SiliconFlowError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(&self.url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "siliconflow user info request failed (transport)");
                SiliconFlowError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "siliconflow user info HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response
            .text()
            .map_err(|_| SiliconFlowError::InvalidResponse)?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(EndpointResponse { status, body })
    }
}

#[cfg(test)]
impl SiliconFlowClient {
    pub fn for_test(url: &str, timeout: Duration) -> Self {
        Self::with_endpoint(url, timeout).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use reqwest::Url;

    use super::SiliconFlowClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    #[test]
    fn provider_proxy_remaining_route_siliconflow() {
        let (url, request) = test_http::serve_once_capturing_request(200, r#"{"data":{}}"#);
        let proxy_url = url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };
        let client = SiliconFlowClient::for_test(&url, Duration::from_secs(1));

        client.fetch(&context, "provider-api-key").unwrap();

        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("authorization: bearer provider-api-key"));
        assert!(request.contains("accept: application/json"));
        assert!(request.contains(concat!("user-agent: quota01/", env!("CARGO_PKG_VERSION"))));
        assert!(request.contains("accept: application/json"));
    }
}
