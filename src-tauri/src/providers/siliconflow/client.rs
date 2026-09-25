use std::time::Duration;

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use super::{SiliconFlowError, Site};

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct SiliconFlowClient {
    client: Client,
    url: String,
}

impl SiliconFlowClient {
    pub fn new(site: Site) -> Result<Self, SiliconFlowError> {
        Self::with_endpoint(site.user_info_url(), Duration::from_secs(15))
    }

    fn with_endpoint(url: &str, timeout: Duration) -> Result<Self, SiliconFlowError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(timeout)
            .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| SiliconFlowError::ConnectionFailed)?;
        Ok(Self {
            client,
            url: url.to_owned(),
        })
    }

    pub fn fetch(&self, api_key: &str) -> Result<EndpointResponse, SiliconFlowError> {
        let started = std::time::Instant::now();
        let response = self
            .client
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
