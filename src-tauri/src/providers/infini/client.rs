use std::time::Duration;

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

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
    client: Client,
    url: String,
}

impl InfiniClient {
    pub fn new() -> Result<Self, InfiniError> {
        Self::with_endpoint(USAGE_URL, Duration::from_secs(15))
    }

    fn with_endpoint(url: &str, timeout: Duration) -> Result<Self, InfiniError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(timeout)
            .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| InfiniError::ConnectionFailed)?;
        Ok(Self {
            client,
            url: url.to_owned(),
        })
    }

    pub fn fetch(&self, api_key: &str) -> Result<EndpointResponse, InfiniError> {
        let started = std::time::Instant::now();
        let response = self
            .client
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
