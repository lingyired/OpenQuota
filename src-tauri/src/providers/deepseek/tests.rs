use std::time::Duration;

use crate::{
    models::{ApiKeyStatus, ProviderErrorKind},
    providers::{
        api_key::{ApiKeyStore, EnvironmentReader, SecretBackend, SecretBytes},
        test_http, UsageProvider,
    },
};

use super::{client::DeepSeekClient, definition, DeepSeekProvider};

const BALANCE_BODY: &str = r#"{
  "is_available": true,
  "balance_infos": [
    {"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"}
  ]
}"#;

struct MemorySecrets(std::sync::Mutex<Option<Vec<u8>>>);

impl SecretBackend for MemorySecrets {
    fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
        Ok(self.0.lock().unwrap().clone().map(SecretBytes::new))
    }

    fn write(&self, _account: &str, value: &[u8]) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(value.to_vec());
        Ok(())
    }

    fn delete(&self, _account: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

struct EmptyEnvironment;

impl EnvironmentReader for EmptyEnvironment {
    fn value(&self, _name: &str) -> Option<String> {
        None
    }
}

fn auth(key: Option<&str>) -> ApiKeyStore {
    let secrets = std::sync::Arc::new(MemorySecrets(std::sync::Mutex::new(
        key.map(|value| value.as_bytes().to_vec()),
    )));
    ApiKeyStore::with_backends(
        "deepseek",
        "DEEPSEEK_API_KEY",
        secrets,
        std::sync::Arc::new(EmptyEnvironment),
    )
}

fn provider(url: &str, key: Option<&str>) -> DeepSeekProvider {
    DeepSeekProvider::with_dependencies(
        auth(key),
        DeepSeekClient::for_test(url, Duration::from_secs(1)),
    )
}

#[test]
fn definition_exposes_balance_and_api_key_configuration() {
    let definition = definition();
    assert_eq!(definition.id, "deepseek");
    assert_eq!(
        definition
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect::<Vec<_>>(),
        ["deepseek.balance", "deepseek.status"]
    );
}

#[test]
fn refresh_maps_the_official_balance_endpoint() {
    let url = test_http::serve_once(200, &[], BALANCE_BODY);
    let provider = provider(&url, Some("sk-test"));

    let snapshot = provider.refresh().unwrap();
    assert_eq!(snapshot.provider_id, "deepseek");
    assert_eq!(snapshot.plan.as_deref(), Some("Available"));
    assert_eq!(snapshot.value_metrics[0].id, "balance");
    assert_eq!(snapshot.value_metrics[0].values[0].number, 110.0);
    assert_eq!(
        snapshot.value_metrics[0].values[0].label.as_deref(),
        Some("CNY")
    );
}

#[test]
fn missing_and_invalid_keys_are_typed_as_authentication_failures() {
    let missing = provider("http://127.0.0.1:1", None).refresh().unwrap_err();
    assert_eq!(missing.kind(), ProviderErrorKind::Authentication);

    let url = test_http::serve_once(401, &[], "Authentication Fails");
    let invalid = provider(&url, Some("sk-bad")).refresh().unwrap_err();
    assert_eq!(invalid.kind(), ProviderErrorKind::Authentication);
}

#[test]
fn unavailable_balance_metadata_is_exposed_without_failing() {
    let url = test_http::serve_once(200, &[], r#"{"is_available":false,"balance_infos":[]}"#);
    let snapshot = provider(&url, Some("sk-test")).refresh().unwrap();
    assert_eq!(snapshot.plan.as_deref(), Some("Unavailable"));
    assert_eq!(snapshot.status_metrics[0].text, "Unavailable");
}

#[test]
fn api_key_status_is_reported() {
    let provider = provider("http://127.0.0.1:1", Some("sk-test"));
    assert_eq!(
        provider.api_key_status().unwrap().unwrap(),
        ApiKeyStatus::Saved
    );
    assert!(provider.supports_api_key_configuration());
}
