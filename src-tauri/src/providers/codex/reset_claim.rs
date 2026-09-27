use std::{collections::HashMap, sync::Mutex};

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::Value;

use super::{auth::CodexAuthState, client::CodexClient};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResetClaimOutcome {
    Success,
    NothingToReset,
    NoCredit,
    Failed,
}

pub struct CodexResetClaimService {
    client: CodexClient,
    matched_credit_ids: Mutex<HashMap<String, String>>,
}

impl CodexResetClaimService {
    pub fn new() -> Result<Self, super::CodexError> {
        Ok(Self {
            client: CodexClient::new()?,
            matched_credit_ids: Mutex::new(HashMap::new()),
        })
    }

    pub fn claim(
        &self,
        context: &crate::providers::ProviderRequestContext,
        expiry: DateTime<Utc>,
        redeem_request_id: &str,
    ) -> ResetClaimOutcome {
        let candidates = match CodexAuthState::load_candidates() {
            Ok(candidates) => candidates,
            Err(_) => {
                crate::app_error!("provider:codex", "reset claim has no usable credentials");
                return ResetClaimOutcome::Failed;
            }
        };
        self.claim_with_candidates(context, expiry, redeem_request_id, &candidates)
    }

    fn claim_with_candidates(
        &self,
        context: &crate::providers::ProviderRequestContext,
        expiry: DateTime<Utc>,
        redeem_request_id: &str,
        candidates: &[CodexAuthState],
    ) -> ResetClaimOutcome {
        let redeem_request_id = redeem_request_id.trim();
        if redeem_request_id.is_empty() || redeem_request_id.len() > 128 {
            return ResetClaimOutcome::Failed;
        }

        let cached_credit_id = self
            .matched_credit_ids
            .lock()
            .ok()
            .and_then(|cache| cache.get(redeem_request_id).cloned());
        let (credit_id, preferred_index) = if let Some(credit_id) = cached_credit_id {
            (credit_id, None)
        } else {
            let mut matched = None;
            for (index, candidate) in candidates.iter().enumerate() {
                let response = match self.client.fetch_reset_credits_with_context(
                    context,
                    &candidate.access_token,
                    candidate.account_id.as_deref(),
                ) {
                    Ok(response) => response,
                    Err(_) => {
                        crate::app_error!("provider:codex", "reset claim list request failed");
                        return ResetClaimOutcome::Failed;
                    }
                };
                if matches!(
                    response.status,
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                ) {
                    continue;
                }
                if !response.status.is_success() || !response.body.is_object() {
                    crate::app_error!(
                        "provider:codex",
                        "reset claim list request returned HTTP {}",
                        response.status.as_u16()
                    );
                    return ResetClaimOutcome::Failed;
                }
                let Some(credit_id) = credit_id_for_expiry(&response.body, expiry) else {
                    return ResetClaimOutcome::NoCredit;
                };
                matched = Some((credit_id, index));
                break;
            }
            let Some((credit_id, index)) = matched else {
                return ResetClaimOutcome::Failed;
            };
            if let Ok(mut cache) = self.matched_credit_ids.lock() {
                if cache.len() >= 256 {
                    cache.clear();
                }
                cache.insert(redeem_request_id.to_owned(), credit_id.clone());
            }
            (credit_id, Some(index))
        };

        let indexes = preferred_index
            .into_iter()
            .chain((0..candidates.len()).filter(|index| Some(*index) != preferred_index));
        for index in indexes {
            let candidate = &candidates[index];
            let response = match self.client.consume_reset_credit_with_context(
                context,
                &candidate.access_token,
                candidate.account_id.as_deref(),
                &credit_id,
                redeem_request_id,
            ) {
                Ok(response) => response,
                Err(_) => {
                    crate::app_error!("provider:codex", "reset claim consume request failed");
                    return ResetClaimOutcome::Failed;
                }
            };
            if matches!(
                response.status,
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
            ) {
                continue;
            }
            let outcome = outcome_from_consume(response.status, &response.body);
            if outcome == ResetClaimOutcome::Failed {
                crate::app_error!(
                    "provider:codex",
                    "reset claim consume returned an unsupported response (HTTP {})",
                    response.status.as_u16()
                );
            }
            return outcome;
        }
        ResetClaimOutcome::Failed
    }
}

fn credit_id_for_expiry(body: &Value, expiry: DateTime<Utc>) -> Option<String> {
    body.get("credits")?.as_array()?.iter().find_map(|credit| {
        if credit
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status != "available")
        {
            return None;
        }
        let candidate = parse_expiry(credit.get("expires_at")?)?;
        (candidate
            .signed_duration_since(expiry)
            .num_milliseconds()
            .abs()
            < 1_000)
            .then(|| credit.get("id")?.as_str().map(str::to_owned))?
    })
}

fn parse_expiry(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(text) = value.as_str() {
        return DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|date| date.to_utc());
    }
    let seconds = value.as_f64().filter(|value| value.is_finite())?;
    DateTime::from_timestamp_millis((seconds * 1_000.0).round() as i64)
}

fn outcome_from_consume(status: StatusCode, body: &Value) -> ResetClaimOutcome {
    if !status.is_success() {
        return ResetClaimOutcome::Failed;
    }
    match body.get("code").and_then(Value::as_str) {
        Some("reset" | "already_redeemed") => ResetClaimOutcome::Success,
        Some("nothing_to_reset") => ResetClaimOutcome::NothingToReset,
        Some("no_credit") => ResetClaimOutcome::NoCredit,
        _ => ResetClaimOutcome::Failed,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, time::Duration};

    use chrono::{TimeZone, Utc};
    use reqwest::StatusCode;
    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        credit_id_for_expiry, outcome_from_consume, CodexResetClaimService, ResetClaimOutcome,
    };
    use crate::providers::{
        codex::{
            auth::{load_from_path_for_test, CodexAuthState},
            client::CodexClient,
        },
        http::ProviderHttpClientFactory,
        test_http, ProviderRequestContext,
    };

    fn test_candidate() -> (tempfile::TempDir, CodexAuthState) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("auth.json");
        fs::write(
            &path,
            r#"{"tokens":{"access_token":"secret-token","account_id":"account-a"}}"#,
        )
        .unwrap();
        let candidate = load_from_path_for_test(&path).unwrap();
        (directory, candidate)
    }

    fn test_service(reset_url: &str, consume_url: &str) -> CodexResetClaimService {
        CodexResetClaimService {
            client: CodexClient::with_test_endpoints(
                "http://codex.invalid/usage",
                reset_url,
                consume_url,
                "http://codex.invalid/token",
                Duration::from_secs(1),
            )
            .unwrap(),
            matched_credit_ids: Default::default(),
        }
    }

    fn list_response(expiry: chrono::DateTime<Utc>) -> String {
        format!(
            r#"{{"credits":[{{"id":"credit-1","status":"available","expires_at":{}}}]}}"#,
            expiry.timestamp()
        )
    }

    #[test]
    fn reset_claim_list_and_consume_requests_use_the_selected_proxy() {
        let expiry = Utc.with_ymd_and_hms(2026, 9, 27, 0, 0, 0).unwrap();
        let list_body = list_response(expiry);
        let (proxy_url, proxy_server) = test_http::serve_sequence_capturing_requests(&[
            (200, list_body.as_str()),
            (200, r#"{"code":"reset"}"#),
        ]);
        let service = test_service(
            "http://codex.invalid/reset-credits",
            "http://codex.invalid/reset-credits/consume",
        );
        let (_directory, candidate) = test_candidate();
        let context = ProviderRequestContext {
            proxy_url: Some(reqwest::Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };

        let outcome = service.claim_with_candidates(
            &context,
            expiry,
            "redeem-1",
            std::slice::from_ref(&candidate),
        );

        assert_eq!(outcome, ResetClaimOutcome::Success);
        let requests = proxy_server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("GET http://codex.invalid/reset-credits HTTP/1.1"));
        assert!(requests[1].starts_with("POST http://codex.invalid/reset-credits/consume HTTP/1.1"));
        for request in &requests {
            let request = request.to_ascii_lowercase();
            assert!(request.contains("authorization: bearer secret-token"));
            assert!(request.contains("chatgpt-account-id: account-a"));
        }
    }

    #[test]
    fn reset_claim_direct_context_keeps_list_and_consume_requests_direct() {
        let expiry = Utc.with_ymd_and_hms(2026, 9, 27, 0, 0, 0).unwrap();
        let list_body = list_response(expiry);
        let (base_url, server) = test_http::serve_sequence_capturing_requests(&[
            (200, list_body.as_str()),
            (200, r#"{"code":"reset"}"#),
        ]);
        let service = test_service(
            &format!("{base_url}/reset-credits"),
            &format!("{base_url}/reset-credits/consume"),
        );
        let (_directory, candidate) = test_candidate();
        let context =
            ProviderRequestContext::direct(Arc::new(ProviderHttpClientFactory::default()));

        let outcome = service.claim_with_candidates(
            &context,
            expiry,
            "redeem-1",
            std::slice::from_ref(&candidate),
        );

        assert_eq!(outcome, ResetClaimOutcome::Success);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("GET /reset-credits HTTP/1.1"));
        assert!(requests[1].starts_with("POST /reset-credits/consume HTTP/1.1"));
    }

    #[test]
    fn matches_only_an_available_credit_at_the_selected_expiry() {
        let expiry = Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).unwrap();
        let body = json!({"credits": [
            {"id": "consumed", "status": "consumed", "expires_at": expiry.timestamp()},
            {"id": "chosen", "expires_at": "2026-07-20T12:00:00.500Z"},
            {"id": "later", "status": "available", "expires_at": expiry.timestamp() + 60}
        ]});

        assert_eq!(
            credit_id_for_expiry(&body, expiry).as_deref(),
            Some("chosen")
        );
    }

    #[test]
    fn maps_idempotent_consume_codes_and_rejects_unknown_responses() {
        for code in ["reset", "already_redeemed"] {
            assert_eq!(
                outcome_from_consume(StatusCode::OK, &json!({"code": code})),
                ResetClaimOutcome::Success
            );
        }
        assert_eq!(
            outcome_from_consume(StatusCode::OK, &json!({"code": "nothing_to_reset"})),
            ResetClaimOutcome::NothingToReset
        );
        assert_eq!(
            outcome_from_consume(StatusCode::OK, &json!({"code": "no_credit"})),
            ResetClaimOutcome::NoCredit
        );
        assert_eq!(
            outcome_from_consume(StatusCode::BAD_GATEWAY, &json!({"code": "reset"})),
            ResetClaimOutcome::Failed
        );
    }
}
