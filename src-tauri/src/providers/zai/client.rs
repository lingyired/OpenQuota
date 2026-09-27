use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::{Site, ZaiError};

#[derive(Debug)]
pub struct ZaiResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct ZaiClient {
    timeout: Duration,
    provider_id: String,
    subscription_url: String,
    quota_url: String,
}

impl ZaiClient {
    pub fn new(site: Site) -> Result<Self, ZaiError> {
        Self::with_endpoints(
            site.subscription_url(),
            site.quota_url(),
            Duration::from_secs(15),
            site.id(),
        )
    }

    fn with_endpoints(
        subscription_url: &str,
        quota_url: &str,
        timeout: Duration,
        provider_id: &str,
    ) -> Result<Self, ZaiError> {
        Ok(Self {
            timeout,
            provider_id: provider_id.to_owned(),
            subscription_url: subscription_url.to_owned(),
            quota_url: quota_url.to_owned(),
        })
    }

    fn http_client(&self, context: &ProviderRequestContext) -> Result<Arc<Client>, ZaiError> {
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
            .map_err(|_| ZaiError::ConnectionFailed)
    }

    pub fn fetch_quota(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<ZaiResponse, ZaiError> {
        self.fetch(context, &self.quota_url, api_key, "quota")
    }

    pub fn fetch_subscription(
        &self,
        context: &ProviderRequestContext,
        api_key: &str,
    ) -> Result<ZaiResponse, ZaiError> {
        self.fetch(context, &self.subscription_url, api_key, "subscription")
    }

    fn fetch(
        &self,
        context: &ProviderRequestContext,
        url: &str,
        api_key: &str,
        endpoint: &str,
    ) -> Result<ZaiResponse, ZaiError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "zai {endpoint} request failed (transport)");
                ZaiError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "zai {endpoint} HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response.text().map_err(|_| ZaiError::InvalidResponse)?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(ZaiResponse { status, body })
    }
}

#[cfg(test)]
impl ZaiClient {
    pub fn for_test(subscription_url: &str, quota_url: &str, timeout: Duration) -> Self {
        Self::with_endpoints(subscription_url, quota_url, timeout, Site::Global.id()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use reqwest::Url;

    use super::ZaiClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    #[test]
    fn provider_proxy_remaining_route_zai() {
        let (url, request) = test_http::serve_once_capturing_request(200, r#"{"data":{}}"#);
        let proxy_url = url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };
        let client = ZaiClient::for_test(&url, &url, Duration::from_secs(1));

        client.fetch_quota(&context, "provider-api-key").unwrap();

        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("authorization: bearer provider-api-key"));
        assert!(request.contains("accept: application/json"));
        assert!(request.contains(concat!("user-agent: quota01/", env!("CARGO_PKG_VERSION"))));
    }
}
