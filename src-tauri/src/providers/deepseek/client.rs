use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::DeepSeekError;

const SUMMARY_URL: &str = "https://platform.deepseek.com/api/v0/users/get_user_summary";
const COST_URL: &str = "https://platform.deepseek.com/api/v0/usage/by_api_key/cost";
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct DeepSeekClient {
    timeout: Duration,
    summary_url: String,
    cost_url: String,
}

impl DeepSeekClient {
    pub fn new() -> Result<Self, DeepSeekError> {
        Self::with_endpoints(SUMMARY_URL, COST_URL, Duration::from_secs(15))
    }

    fn with_endpoints(
        summary_url: &str,
        cost_url: &str,
        timeout: Duration,
    ) -> Result<Self, DeepSeekError> {
        Ok(Self {
            timeout,
            summary_url: summary_url.to_owned(),
            cost_url: cost_url.to_owned(),
        })
    }

    fn http_client(&self, context: &ProviderRequestContext) -> Result<Arc<Client>, DeepSeekError> {
        context
            .http_clients
            .client(
                "deepseek",
                "default",
                context.proxy_url.as_ref(),
                |builder| {
                    builder
                        .connect_timeout(Duration::from_secs(8))
                        .timeout(self.timeout)
                        .user_agent(BROWSER_USER_AGENT)
                },
            )
            .map_err(|_| DeepSeekError::ConnectionFailed)
    }

    pub fn fetch_summary(
        &self,
        context: &ProviderRequestContext,
        user_token: &str,
    ) -> Result<EndpointResponse, DeepSeekError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(&self.summary_url)
            .bearer_auth(user_token)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "deepseek summary request failed (transport)");
                DeepSeekError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "deepseek summary HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        decode(response, status)
    }

    pub fn fetch_today_cost(
        &self,
        context: &ProviderRequestContext,
        user_token: &str,
        start: i64,
        end: i64,
        timezone: i32,
    ) -> Result<EndpointResponse, DeepSeekError> {
        let client = self.http_client(context)?;
        let started = std::time::Instant::now();
        let response = client
            .get(&self.cost_url)
            .query(&[
                ("start", start.to_string()),
                ("end", end.to_string()),
                ("tz", timezone.to_string()),
            ])
            .bearer_auth(user_token)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "deepseek today cost request failed (transport)");
                DeepSeekError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "deepseek today cost HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        decode(response, status)
    }
}

fn decode(
    response: reqwest::blocking::Response,
    status: StatusCode,
) -> Result<EndpointResponse, DeepSeekError> {
    let text = response
        .text()
        .map_err(|_| DeepSeekError::InvalidResponse)?;
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok(EndpointResponse { status, body })
}

#[cfg(test)]
impl DeepSeekClient {
    pub fn for_test(summary_url: &str, cost_url: &str, timeout: Duration) -> Self {
        Self::with_endpoints(summary_url, cost_url, timeout).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use reqwest::Url;

    use super::DeepSeekClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    #[test]
    fn provider_proxy_api_key_route_deepseek() {
        let (base_url, request) =
            test_http::serve_once_capturing_request(200, r#"{"marker":"proxy"}"#);
        let proxy_url = base_url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let client = DeepSeekClient::for_test(
            &format!("{base_url}/summary"),
            &format!("{base_url}/cost"),
            Duration::from_secs(1),
        );
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };

        let response = client.fetch_summary(&context, "provider-api-key").unwrap();

        assert_eq!(response.body["marker"], "proxy");
        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("authorization: bearer provider-api-key"));
    }
}
