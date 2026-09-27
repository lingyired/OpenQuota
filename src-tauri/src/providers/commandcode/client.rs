use std::time::Duration;

use reqwest::{blocking::Client, StatusCode};
use serde_json::Value;

use super::CommandCodeError;

const API_BASE: &str = "https://api.commandcode.ai";
const CREDITS_PATH: &str = "/alpha/billing/credits";
const SUBSCRIPTIONS_PATH: &str = "/alpha/billing/subscriptions";

#[derive(Debug, Default)]
pub(super) struct BillingResponse {
    pub(super) status: StatusCode,
    pub(super) body: Value,
}

pub(super) struct CommandCodeClient {
    client: Client,
    api_base: String,
}

impl CommandCodeClient {
    pub(super) fn new() -> Result<Self, CommandCodeError> {
        Self::with_base_url(None)
    }

    fn with_base_url(test_base_url: Option<String>) -> Result<Self, CommandCodeError> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(8))
                .timeout(Duration::from_secs(15))
                .user_agent(concat!("Quota01/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|_| CommandCodeError::ConnectionFailed)?,
            api_base: test_base_url
                .unwrap_or_else(|| API_BASE.to_owned())
                .trim_end_matches('/')
                .to_owned(),
        })
    }

    pub(super) fn fetch_credits(&self, api_key: &str) -> Result<BillingResponse, CommandCodeError> {
        self.fetch(CREDITS_PATH, api_key, "credits")
    }

    /// The subscription carries the plan name (Go / GOAT / Pro / Max / Ultra)
    /// and the monthly reset date, but the endpoint is secondary: a failure
    /// there degrades to a generic report instead of failing the refresh.
    pub(super) fn fetch_subscription(
        &self,
        api_key: &str,
    ) -> Result<BillingResponse, CommandCodeError> {
        self.fetch(SUBSCRIPTIONS_PATH, api_key, "subscription")
    }

    fn fetch(
        &self,
        path: &str,
        api_key: &str,
        endpoint: &str,
    ) -> Result<BillingResponse, CommandCodeError> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!("{}{path}", self.api_base))
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "commandcode {endpoint} request failed (transport)");
                CommandCodeError::ConnectionFailed
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "commandcode {endpoint} HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let body = response
            .text()
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        Ok(BillingResponse { status, body })
    }
}

#[cfg(test)]
impl CommandCodeClient {
    pub(super) fn for_test(base_url: &str) -> Self {
        Self::with_base_url(Some(base_url.to_owned()))
            .expect("test CommandCode endpoint should be valid")
    }
}
