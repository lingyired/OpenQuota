use std::time::Duration;

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use super::DeepSeekError;

const BALANCE_URL: &str = "https://api.deepseek.com/user/balance";

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct DeepSeekClient {
    client: Client,
    balance_url: String,
}

impl DeepSeekClient {
    pub fn new() -> Result<Self, DeepSeekError> {
        Self::with_endpoint(BALANCE_URL, Duration::from_secs(15))
    }

    fn with_endpoint(url: &str, timeout: Duration) -> Result<Self, DeepSeekError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(timeout)
            .user_agent(concat!("Usage01/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| DeepSeekError::ConnectionFailed)?;
        Ok(Self {
            client,
            balance_url: url.to_owned(),
        })
    }

    pub fn fetch_balance(&self, api_key: &str) -> Result<EndpointResponse, DeepSeekError> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(&self.balance_url)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "deepseek balance request failed (transport)");
                DeepSeekError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "deepseek balance HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response
            .text()
            .map_err(|_| DeepSeekError::InvalidResponse)?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(EndpointResponse { status, body })
    }
}

#[cfg(test)]
impl DeepSeekClient {
    pub fn for_test(url: &str, timeout: Duration) -> Self {
        Self::with_endpoint(url, timeout).unwrap()
    }
}
