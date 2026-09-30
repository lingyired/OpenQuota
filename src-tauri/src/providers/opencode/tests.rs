use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use tempfile::tempdir;

use crate::{
    models::{ApiKeyStatus, MetricSection, ProviderErrorKind},
    providers::{api_key::*, test_http, UsageProvider},
};

use super::{
    auth::OpenCodeAuthStore, client::OpenCodeClient, definition, OpenCodeError, OpenCodeProvider,
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

struct UnreadableSecrets;

impl SecretBackend for UnreadableSecrets {
    fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
        Err("System credential store unavailable.".into())
    }
    fn write(&self, _account: &str, _value: &[u8]) -> Result<(), String> {
        Err("System credential store unavailable.".into())
    }
    fn delete(&self, _account: &str) -> Result<(), String> {
        Err("System credential store unavailable.".into())
    }
}

fn auth_with_store(
    secrets: Arc<dyn SecretBackend>,
    key: Option<&str>,
    data_directory: std::path::PathBuf,
) -> OpenCodeAuthStore {
    let store = ApiKeyStore::with_backends(
        "opencode",
        "OPENCODE_GO_API_KEY",
        secrets,
        Arc::new(Environment(
            key.map(|value| HashMap::from([("OPENCODE_GO_API_KEY".into(), value.into())]))
                .unwrap_or_default(),
        )),
    );
    OpenCodeAuthStore::with_store(store, data_directory)
}

fn auth(key: Option<&str>, data_directory: std::path::PathBuf) -> OpenCodeAuthStore {
    auth_with_store(Arc::new(MemorySecrets::default()), key, data_directory)
}

/// Mirrors what the `opencode` CLI leaves behind after signing in to a Go plan.
fn write_cli_sign_in(directory: &std::path::Path, key: &str) {
    std::fs::write(
        directory.join("auth.json"),
        format!(r#"{{"$schema":"v1","opencode-go":{{"type":"api","key":"{key}"}}}}"#),
    )
    .unwrap();
}

fn client(url: &str) -> OpenCodeClient {
    OpenCodeClient::for_test(url, Duration::from_secs(1))
}

const QUOTA_BODY: &str = r#"{"usage":{
    "rolling":{"percent":31,"resetsAt":"2026-08-12T12:00:00Z","status":"ok"},
    "weekly":{"percent":100,"resetsAt":"2026-08-17T00:00:00Z","status":"rate-limited"},
    "monthly":{"percent":72,"resetsAt":"2026-09-05T00:00:00Z","status":"ok"}}}"#;

#[test]
fn definition_exposes_the_go_quota_contract() {
    let mut definition = definition();
    crate::providers::normalize_default_pins(&mut definition.metrics);
    assert_eq!(definition.id, "opencode");
    assert_eq!(definition.display_name, "OpenCode Go");
    assert!(definition.local_usage_source_note.is_none());
    assert_eq!(
        definition
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect::<Vec<_>>(),
        ["opencode.session", "opencode.weekly", "opencode.monthly"]
    );
    assert!(definition
        .metrics
        .iter()
        .all(|metric| metric.default_section == MetricSection::AlwaysVisible));
    // OpenCode Go reports floored whole percentages, so `0` must not read as "unused".
    assert!(definition
        .metrics
        .iter()
        .all(|metric| metric.floored_percent));
    assert!(definition.metrics[0].source.session_window());
    assert!(!definition.metrics[1].source.session_window());
    assert!(!definition.metrics[2].source.session_window());
    // normalize_default_pins stars the first two eligible metrics.
    assert!(definition.metrics[0].default_pinned);
    assert!(definition.metrics[1].default_pinned);
    assert!(!definition.metrics[2].default_pinned);
}

#[test]
fn refresh_maps_the_go_usage_windows() {
    let directory = tempdir().unwrap();
    let url = test_http::serve_once(200, &[], QUOTA_BODY);
    let provider = OpenCodeProvider::with_dependencies(
        auth(Some("secret"), directory.path().into()),
        client(&url),
    );

    let snapshot = provider.refresh().unwrap();

    assert_eq!(snapshot.provider_id, "opencode");
    assert_eq!(snapshot.plan.as_deref(), Some("Go"));
    assert_eq!(
        snapshot
            .quotas
            .iter()
            .map(|quota| quota.id.as_str())
            .collect::<Vec<_>>(),
        ["session", "weekly", "monthly"]
    );
    assert_eq!(snapshot.quotas[0].used_percent, 31.0);
    assert_eq!(snapshot.quotas[1].used_percent, 100.0);
    assert_eq!(snapshot.quotas[2].period_seconds, 30 * 24 * 60 * 60);
    assert!(snapshot.quotas[2].resets_at.is_some());
    assert!(snapshot.usage.today.is_none());
    assert!(snapshot.warnings.is_empty());
}

#[test]
fn missing_and_invalid_keys_are_distinct_authentication_errors() {
    let directory = tempdir().unwrap();

    let missing = OpenCodeProvider::with_dependencies(
        auth(None, directory.path().into()),
        client(&test_http::serve_once(200, &[], QUOTA_BODY)),
    )
    .refresh()
    .unwrap_err();
    assert_eq!(missing.kind(), ProviderErrorKind::Authentication);

    let invalid_directory = tempdir().unwrap();
    let invalid = OpenCodeProvider::with_dependencies(
        auth(Some("bad-key"), invalid_directory.path().into()),
        client(&test_http::serve_once(401, &[], "{}")),
    )
    .refresh()
    .unwrap_err();
    assert_eq!(invalid.kind(), ProviderErrorKind::Authentication);
    assert!(!invalid.to_string().contains("bad-key"));
}

#[test]
fn go_subscription_required_is_reported_as_permission() {
    let directory = tempdir().unwrap();
    let url = test_http::serve_once(
        403,
        &[],
        r#"{"type":"error","error":{"type":"EntitlementError","message":"OpenCode Go subscription required."}}"#,
    );
    let provider = OpenCodeProvider::with_dependencies(
        auth(Some("secret"), directory.path().into()),
        client(&url),
    );

    let error = provider.refresh().unwrap_err();

    assert_eq!(error.kind(), ProviderErrorKind::Permission);
    assert_eq!(error.to_string(), "OpenCode Go subscription required.");
}

#[test]
fn transient_failures_map_to_network_and_rate_limit_categories() {
    let directory = tempdir().unwrap();
    let rate_limited = OpenCodeProvider::with_dependencies(
        auth(Some("secret"), directory.path().into()),
        client(&test_http::serve_once(429, &[], r#"{"type":"error"}"#)),
    )
    .refresh()
    .unwrap_err();
    assert_eq!(rate_limited.kind(), ProviderErrorKind::RateLimited);

    let other_directory = tempdir().unwrap();
    let server_failure = OpenCodeProvider::with_dependencies(
        auth(Some("secret"), other_directory.path().into()),
        client(&test_http::serve_once(503, &[], r#"{"type":"error"}"#)),
    )
    .refresh()
    .unwrap_err();
    assert_eq!(server_failure.kind(), ProviderErrorKind::Network);
}

#[test]
fn go_key_falls_back_to_the_opencode_auth_file() {
    let directory = tempdir().unwrap();
    std::fs::write(
        directory.path().join("auth.json"),
        r#"{"$schema":"v1","opencode-go":{"type":"api","key":"  file-key  "}}"#,
    )
    .unwrap();
    let url = test_http::serve_once(200, &[], QUOTA_BODY);
    let provider =
        OpenCodeProvider::with_dependencies(auth(None, directory.path().into()), client(&url));

    assert!(provider.has_local_credentials());
    assert_eq!(
        provider.api_key_status().unwrap().unwrap(),
        ApiKeyStatus::FromCliSignIn
    );
    let snapshot = provider.refresh().unwrap();
    assert_eq!(snapshot.plan.as_deref(), Some("Go"));
    assert_eq!(snapshot.quotas.len(), 3);
}

#[test]
fn a_cli_sign_in_is_reported_as_its_own_key_source_and_stays_overridable() {
    let directory = tempdir().unwrap();
    write_cli_sign_in(directory.path(), "file-key");
    let store = auth(None, directory.path().into());

    // Nothing is saved in Quota01, so the CLI credential is named as the source instead of the
    // card reading as "no key".
    assert_eq!(store.status().unwrap(), ApiKeyStatus::FromCliSignIn);

    store.save("app-key").unwrap();
    assert_eq!(store.status().unwrap(), ApiKeyStatus::Saved);
    assert_eq!(store.load().unwrap().unwrap().as_str(), "app-key");

    // Removing the app key falls back to the CLI credential rather than dropping the card.
    store.delete().unwrap();
    assert_eq!(store.status().unwrap(), ApiKeyStatus::FromCliSignIn);
    assert_eq!(store.load().unwrap().unwrap().as_str(), "file-key");
}

#[test]
fn a_cli_sign_in_keeps_working_when_the_system_store_cannot_be_read() {
    let directory = tempdir().unwrap();
    write_cli_sign_in(directory.path(), "file-key");
    let store = auth_with_store(Arc::new(UnreadableSecrets), None, directory.path().into());

    assert!(store.has_local_credentials());
    assert_eq!(store.status().unwrap(), ApiKeyStatus::FromCliSignIn);
    assert_eq!(store.load().unwrap().unwrap().as_str(), "file-key");
}

#[test]
fn an_unreadable_store_without_a_cli_sign_in_is_a_storage_error() {
    let directory = tempdir().unwrap();
    let store = auth_with_store(Arc::new(UnreadableSecrets), None, directory.path().into());

    // The store's own message must survive: it is what lets an orphaned vault be
    // reclassified instead of reading as a missing API key.
    const UNAVAILABLE: &str = "System credential store unavailable.";

    assert!(!store.has_local_credentials());
    assert_eq!(
        store.status().unwrap_err(),
        OpenCodeError::CredentialStorage(UNAVAILABLE.into())
    );
    assert_eq!(
        store.load().unwrap_err(),
        OpenCodeError::CredentialStorage(UNAVAILABLE.into())
    );
}

#[test]
fn no_key_at_all_is_absent_credentials() {
    let directory = tempdir().unwrap();
    let provider = OpenCodeProvider::with_dependencies(
        auth(None, directory.path().into()),
        client("http://127.0.0.1:1"),
    );

    assert!(!provider.has_local_credentials());
    assert_eq!(
        provider.api_key_status().unwrap().unwrap(),
        ApiKeyStatus::NotSet
    );
    assert_eq!(
        provider.refresh().unwrap_err().kind(),
        ProviderErrorKind::Authentication
    );
}
