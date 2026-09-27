use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde_json::json;

use crate::{
    models::{ApiKeyStatus, ProviderErrorKind},
    providers::{api_key::*, test_http, UsageProvider},
};

use super::{
    auth::CommandCodeAuthStore, client::CommandCodeClient, definition, CommandCodeProvider,
};

#[derive(Default)]
struct MemorySecrets(Mutex<HashMap<String, Vec<u8>>>);

impl SecretBackend for MemorySecrets {
    fn read(&self, account: &str) -> Result<Option<SecretBytes>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(account)
            .cloned()
            .map(SecretBytes::new))
    }
    fn write(&self, account: &str, value: &[u8]) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert(account.to_owned(), value.to_vec());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

struct Environment(HashMap<String, String>);
impl EnvironmentReader for Environment {
    fn value(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned()
    }
}

fn auth(key: Option<&str>) -> CommandCodeAuthStore {
    CommandCodeAuthStore::with_store(ApiKeyStore::with_backends(
        "commandcode",
        "COMMANDCODE_API_KEY",
        Arc::new(MemorySecrets::default()),
        Arc::new(Environment(
            key.map(|value| HashMap::from([("COMMANDCODE_API_KEY".to_owned(), value.to_owned())]))
                .unwrap_or_default(),
        )),
    ))
}

fn credits_body(monthly: &str) -> String {
    json!({
        "credits": {"monthlyCredits": monthly, "purchasedCredits": 0, "freeCredits": 0},
        "windowLimits": {"limited": false}
    })
    .to_string()
}

/// The refresh always hits the credits endpoint first and the optional
/// subscriptions endpoint second, so one sequential server backs both.
fn provider(key: Option<&str>, responses: &[(u16, &str)]) -> CommandCodeProvider {
    let base_url = test_http::serve_sequence(responses);
    CommandCodeProvider::with_dependencies(auth(key), CommandCodeClient::for_test(&base_url))
}

const GOAT_SUBSCRIPTION: &str = r#"{"success":true,"data":{"planId":"individual-goat","currentPeriodEnd":"2026-10-01T00:00:00"}}"#;

#[test]
fn refresh_maps_the_goat_plan_monthly_and_windows() {
    let body = json!({
        "credits": {"monthlyCredits": "23.5", "purchasedCredits": 5, "freeCredits": 0},
        "windowLimits": {
            "limited": true,
            "fiveHour": {"used": 28, "cap": 70, "resetAt": 1800000000000i64},
            "weekly": {"used": 35, "cap": 70}
        }
    })
    .to_string();
    let snapshot = provider(Some("cc-secret"), &[(200, &body), (200, GOAT_SUBSCRIPTION)])
        .refresh()
        .unwrap();

    assert_eq!(snapshot.provider_id, "commandcode");
    assert_eq!(snapshot.plan.as_deref(), Some("GOAT"));
    assert!(snapshot.value_metrics.is_empty());
    assert_eq!(
        snapshot
            .quotas
            .iter()
            .map(|quota| (quota.id.as_str(), quota.used_percent))
            .collect::<Vec<_>>(),
        [
            ("monthly", 66.428_571_428_571_43),
            ("session", 40.0),
            ("weekly", 50.0)
        ]
    );
    assert_eq!(snapshot.quotas[0].limit_value, Some(70.0));
    assert_eq!(snapshot.quotas[0].used_value, Some(46.5));
    assert!(snapshot.quotas[1].resets_at.is_some());
}

#[test]
fn an_unknown_plan_degrades_to_estimated_monthly() {
    let snapshot = provider(
        Some("cc-secret"),
        &[
            (200, &credits_body("30")),
            (200, r#"{"success":true,"data":{"planId":"mystery"}}"#),
        ],
    )
    .refresh()
    .unwrap();

    assert_eq!(snapshot.plan, None);
    let monthly = &snapshot.quotas[0];
    assert!(monthly.estimated);
    assert_eq!(monthly.used_value, None);
}

#[test]
fn missing_key_reports_authentication_with_the_env_hint() {
    let error = provider(None, &[(200, &credits_body("30"))])
        .refresh()
        .unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Authentication);
    assert!(error.to_string().contains("COMMANDCODE_API_KEY"));
}

#[test]
fn a_rejected_key_is_an_authentication_error() {
    let error = provider(
        Some("cc-wrong"),
        &[
            (401, r#"{"message":"Invalid API key"}"#),
            (200, GOAT_SUBSCRIPTION),
        ],
    )
    .refresh()
    .unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Authentication);
    assert!(error.to_string().contains("commandcode.ai"));
}

#[test]
fn rate_limited_and_broken_responses_are_distinct() {
    let rate_limited = provider(Some("cc-secret"), &[(429, "{}")])
        .refresh()
        .unwrap_err();
    assert_eq!(rate_limited.kind(), ProviderErrorKind::RateLimited);

    let empty = provider(Some("cc-secret"), &[(200, "{}")])
        .refresh()
        .unwrap_err();
    assert_eq!(empty.kind(), ProviderErrorKind::InvalidResponse);
}

#[test]
fn definition_exposes_expected_identity_and_metrics() {
    let definition = definition();
    assert_eq!(definition.id, "commandcode");
    assert_eq!(definition.display_name, "CommandCode");
    assert!(!definition.fallback_enabled);
    assert_eq!(
        definition
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect::<Vec<_>>(),
        [
            "commandcode.monthly",
            "commandcode.session",
            "commandcode.weekly"
        ]
    );
    assert_eq!(
        definition
            .links
            .iter()
            .map(|link| link.url.as_str())
            .collect::<Vec<_>>(),
        ["https://commandcode.ai/dashboard"]
    );
}

#[test]
fn api_key_capability_delegates_without_exposing_the_secret() {
    let provider = provider(Some("environment"), &[(200, &credits_body("30"))]);
    assert_eq!(
        provider.api_key_status().unwrap().unwrap(),
        ApiKeyStatus::FromEnvironment
    );
    provider.save_api_key("saved").unwrap();
    assert_eq!(
        provider.api_key_status().unwrap().unwrap(),
        ApiKeyStatus::OverrideActive
    );
    provider.delete_api_key().unwrap();
    assert_eq!(
        provider.api_key_status().unwrap().unwrap(),
        ApiKeyStatus::FromEnvironment
    );
}
