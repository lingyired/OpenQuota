use std::time::Duration;

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

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
    client: Client,
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
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(timeout)
            .user_agent(BROWSER_USER_AGENT)
            .build()
            .map_err(|_| DeepSeekError::ConnectionFailed)?;
        Ok(Self {
            client,
            summary_url: summary_url.to_owned(),
            cost_url: cost_url.to_owned(),
        })
    }

    pub fn fetch_summary(&self, user_token: &str) -> Result<EndpointResponse, DeepSeekError> {
        let started = std::time::Instant::now();
        let response = self
            .client
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
        user_token: &str,
        start: i64,
        end: i64,
        timezone: i32,
    ) -> Result<EndpointResponse, DeepSeekError> {
        let started = std::time::Instant::now();
        let response = self
            .client
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
