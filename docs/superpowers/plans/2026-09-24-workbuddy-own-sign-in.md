# WorkBuddy Self-Contained Sign-In Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Quota01 a WorkBuddy credential it obtains and owns itself through a device-code sign-in, so credit and usage tracking no longer depends on reading an encrypted file or on any third-party application.

**Architecture:** A provider-local device-code flow (`auth/state` → browser authorization → `auth/token` poll → `login/account` profile) stores a session in Quota01's existing encrypted vault. Credential loading prefers that session, then the legacy plaintext login file. A new catalog list drives a dedicated sign-in panel in the Customize and Dashboard surfaces. The interim `~/.wb-switch/accounts.json` borrow is removed.

**Tech Stack:** Rust 2021, Tauri 2, reqwest (blocking), serde/serde_json, thiserror, Svelte 5 (runes), TypeScript, Vitest, svelte-i18n.

**Spec:** `docs/superpowers/specs/2026-09-24-workbuddy-own-sign-in-design.md`

## Global Constraints

- Every production change starts from a failing test. Watch it fail before implementing.
- `cargo fmt --all` clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test --all-targets` green.
- `corepack pnpm lint`, `check`, `test`, `build` and `verify:contracts` green.
- Never log, cache, or include in a snapshot: access tokens, refresh tokens, or raw response bodies.
- New user-facing strings go through the i18n system in all 16 locale files under `src/lib/i18n/messages/`, with real translations for `zh-CN` and `zh-TW` and English elsewhere; backend strings are mapped in `src/lib/i18n/backendGlossary.ts`.
- Adding a field to `ProviderCatalog` requires the matching field in the `ProviderCatalog` interface in `src/lib/types.ts` (`scripts/verify/verify-model-contract.js` enforces this).
- Adding a frontend `invoke('…')` requires the matching command in the `tauri::generate_handler![…]` block in `src-tauri/src/lib.rs` (`scripts/verify/verify-command-contract.js` enforces this).
- WorkBuddy endpoints are fixed: base `https://www.codebuddy.cn`, prefix `/v2/plugin`, platform `workbuddy`.

---

## File Structure

**Create**

- `src-tauri/src/providers/workbuddy/session.rs` — the `WorkBuddySession` document and its vault-backed store.
- `src-tauri/src/providers/workbuddy/login.rs` — the device-code state machine and its wire parsing.
- `src-tauri/src/commands/provider_login.rs` — the three Tauri commands that dispatch to any provider supporting device-code sign-in.
- `src/lib/ProviderDeviceCodeLogin.svelte` — the sign-in panel.
- `src/lib/ProviderDeviceCodeLogin.test.ts` — component tests.
- `src-tauri/src/providers/workbuddy/login_tests.rs` — device-code state machine tests (kept beside the module it covers, matching the provider's existing `tests.rs` style).

**Modify**

- `src-tauri/src/providers/workbuddy/auth.rs` — drop the interim shared-store borrow; keep envelope detection; accept a session-first credential source.
- `src-tauri/src/providers/workbuddy/mod.rs` — credential precedence, session hooks, device-code hooks, error mapping.
- `src-tauri/src/providers/mod.rs` — the device-code trait hook and its `DeviceCodeAuth` descriptor.
- `src-tauri/src/models.rs` — `DeviceCodeChallenge`, `DeviceCodePoll`, and `ProviderCatalog.device_code_sign_in_provider_ids`.
- `src-tauri/src/providers/registry.rs` — populate the new catalog list.
- `src-tauri/src/lib.rs` — register the three commands and their state.
- `src/lib/backend.ts`, `src/lib/types.ts`, `src/lib/metrics.ts` — command bindings, types, catalog index helpers.
- `src/lib/CustomizeProviderDetail.svelte`, `src/lib/Dashboard.svelte` — render the new panel for providers that support device-code sign-in.
- `src/lib/i18n/messages/*.ts` (16 files), `src/lib/i18n/backendGlossary.ts` — copy.
- `docs/providers/workbuddy-cn.md` — document the sign-in.

---

### Task 1: Remove the interim shared-store borrow

The rejected interim change borrowed a plaintext token from `~/.wb-switch/accounts.json`. Remove it, keeping the parts that are still correct: envelope detection, the `Encrypted` auth error, the provider-facing `CredentialsEncrypted` error, and the `has_local_credentials` revision.

**Files:**

- Modify: `src-tauri/src/providers/workbuddy/auth.rs`
- Modify: `src-tauri/src/providers/workbuddy/mod.rs`
- Test: the `mod tests` blocks inside both files

**Interfaces:**

- Consumes: nothing.
- Produces: `WorkBuddyAuth::load() -> Result<WorkBuddyAuth, WorkBuddyAuthError>`, `WorkBuddyAuth::load_from_path(&Path) -> Result<WorkBuddyAuth, WorkBuddyAuthError>`, `WorkBuddyAuth::has_local_credentials() -> bool`, `WorkBuddyAuth::has_local_credentials_at(&Path) -> bool`, `WorkBuddyAuthError::{NotLoggedIn, Invalid, Storage, Encrypted}`, `WorkBuddyError::CredentialsEncrypted`.

- [ ] **Step 1: Replace the shared-store tests with tests for the retained behavior**

In `src-tauri/src/providers/workbuddy/auth.rs`, delete the tests `encrypted_auth_file_falls_back_to_the_readable_shared_store`, `encrypted_auth_file_without_a_readable_copy_reports_encryption`, `plaintext_auth_file_wins_over_the_shared_store`, and `shared_store_credentials_are_never_written_back`, plus the `write_store` helper. Then add:

```rust
    #[test]
    fn an_encrypted_login_file_reports_encryption_instead_of_signing_out() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workbuddy-desktop.info");
        fs::write(&path, encrypted_auth_document("uid-1")).unwrap();

        assert!(matches!(
            WorkBuddyAuth::load_from_path(&path),
            Err(WorkBuddyAuthError::Encrypted)
        ));
    }
```

Keep `an_encrypted_login_file_still_counts_as_local_credentials` and `encrypted_auth_document` unchanged.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy::auth`
Expected: FAIL — `load_from_path` still consults the real shared store, so the assertion reports `Ok(..)` or a different error.

- [ ] **Step 3: Remove the borrow from `auth.rs`**

Delete `WorkBuddyAuthSource`, the `source` field on `WorkBuddyAuth`, `is_read_only()`, `load_from_paths()`, `shared_store_path()`, `from_shared_store()`, and the `is_read_only` early return inside `save_tokens()`. Restore `load_from_path` to construct the struct directly, keeping the envelope branch:

```rust
    pub fn load_from_path(path: &Path) -> Result<Self, WorkBuddyAuthError> {
        let text = fs::read_to_string(path).map_err(|_| WorkBuddyAuthError::NotLoggedIn)?;
        let document: Value =
            serde_json::from_str(&text).map_err(|_| WorkBuddyAuthError::Invalid)?;
        let Some(access_token) =
            first_string(&document, ACCESS_TOKEN_PATHS).filter(|value| !value.is_empty())
        else {
            return if is_encrypted_envelope(first_value(&document, ACCESS_TOKEN_PATHS)) {
                Err(WorkBuddyAuthError::Encrypted)
            } else {
                Err(WorkBuddyAuthError::NotLoggedIn)
            };
        };
        Ok(Self {
            path: path.to_owned(),
            access_token,
            refresh_token: first_string(&document, REFRESH_TOKEN_PATHS),
            token_type: first_string(&document, TOKEN_TYPE_PATHS)
                .unwrap_or_else(|| "Bearer".into()),
            domain: first_string(&document, DOMAIN_PATHS).unwrap_or_default(),
            uid: first_string(&document, UID_PATHS),
            enterprise_id: first_string(&document, ENTERPRISE_ID_PATHS),
            nickname: first_string(&document, NICKNAME_PATHS),
            email: first_string(&document, EMAIL_PATHS),
            expires_at: first_value(&document, EXPIRES_AT_PATHS).and_then(parse_datetime),
            document,
        })
    }
```

Extract the repeated path arrays into the named constants used above (`REFRESH_TOKEN_PATHS`, `TOKEN_TYPE_PATHS`, `DOMAIN_PATHS`, `UID_PATHS`, `ENTERPRISE_ID_PATHS`, `NICKNAME_PATHS`, `EMAIL_PATHS`, `EXPIRES_AT_PATHS`) beside the existing `ACCESS_TOKEN_PATHS`.

- [ ] **Step 4: Remove the provider-side read-only machinery**

In `src-tauri/src/providers/workbuddy/mod.rs`, delete the `skip_read_only_refresh` function, its two call sites, the `auth.is_read_only()` guards in `refresh_with_identity`, `refresh_auth`, and `finish_usage`, and the tests `borrowed_credentials`, `borrowed_credentials_skip_refresh_and_warn_exactly_once`, and `regular_credentials_still_refresh`. Remove `WorkBuddyAuth` from that test module's `use super::{…}` list if it becomes unused.

- [ ] **Step 5: Run the provider tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy`
Expected: PASS.

- [ ] **Step 6: Run the whole gate and commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
git add src-tauri/src/providers/workbuddy/auth.rs src-tauri/src/providers/workbuddy/mod.rs
git commit -m "refactor(workbuddy): drop the workbuddy-switch credential borrow"
```

---

### Task 2: The WorkBuddy session document and its store

**Files:**

- Create: `src-tauri/src/providers/workbuddy/session.rs`
- Modify: `src-tauri/src/providers/workbuddy/mod.rs` (add `mod session;`)

**Interfaces:**

- Consumes: `ApiKeyStore`, `ApiKeyStatus`, and `SecretString` from `crate::providers::api_key`.
- Produces:
  - `pub struct WorkBuddySession { pub access_token: String, pub refresh_token: Option<String>, pub token_type: String, pub domain: String, pub uid: Option<String>, pub nickname: Option<String>, pub email: Option<String>, pub enterprise_id: Option<String>, pub expires_at: Option<i64>, pub refresh_expires_at: Option<i64> }`
  - `impl WorkBuddySession { pub fn to_json(&self) -> Result<String, WorkBuddySessionError>; pub fn from_json(text: &str) -> Result<Self, WorkBuddySessionError>; pub fn is_expired(&self, now_ms: i64) -> bool }`
  - `pub struct WorkBuddySessionStore` with `new()`, `load() -> Result<Option<WorkBuddySession>, WorkBuddySessionError>`, `save(&str) -> Result<(), WorkBuddySessionError>`, `delete() -> Result<(), WorkBuddySessionError>`, `status() -> Result<ApiKeyStatus, WorkBuddySessionError>`
  - `pub enum WorkBuddySessionError { Storage, Malformed }`

- [ ] **Step 1: Write the failing tests**

Append to `session.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> WorkBuddySession {
        WorkBuddySession {
            access_token: "access-1".into(),
            refresh_token: Some("refresh-1".into()),
            token_type: "Bearer".into(),
            domain: "www.codebuddy.cn".into(),
            uid: Some("uid-1".into()),
            nickname: Some("Ling".into()),
            email: None,
            enterprise_id: None,
            expires_at: Some(1_800_000_000_000),
            refresh_expires_at: None,
        }
    }

    #[test]
    fn round_trips_through_json_without_losing_fields() {
        let text = session().to_json().unwrap();
        assert_eq!(WorkBuddySession::from_json(&text).unwrap(), session());
    }

    #[test]
    fn rejects_a_document_without_an_access_token() {
        assert!(matches!(
            WorkBuddySession::from_json(r#"{"refresh_token":"r"}"#),
            Err(WorkBuddySessionError::Malformed)
        ));
        assert!(matches!(
            WorkBuddySession::from_json(r#"{"access_token":"  "}"#),
            Err(WorkBuddySessionError::Malformed)
        ));
    }

    #[test]
    fn reports_expiry_against_the_supplied_clock() {
        let mut subject = session();
        subject.expires_at = Some(1_000);
        assert!(subject.is_expired(1_500));
        assert!(!subject.is_expired(500));
        subject.expires_at = None;
        assert!(
            !subject.is_expired(i64::MAX),
            "a session without an expiry stays usable rather than being discarded"
        );
    }

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

    /// 内存后端，绝不触碰用户真实的加密 vault（沿用 deepseek / trae 测试的做法）。
    fn store(value: Option<&str>) -> WorkBuddySessionStore {
        let secrets = std::sync::Arc::new(MemorySecrets(std::sync::Mutex::new(
            value.map(|text| text.as_bytes().to_vec()),
        )));
        WorkBuddySessionStore::with_store(ApiKeyStore::with_backends(
            VAULT_ACCOUNT,
            "WORKBUDDY_SESSION",
            secrets,
            std::sync::Arc::new(EmptyEnvironment),
        ))
    }

    #[test]
    fn store_round_trip_and_delete_use_the_vault() {
        let subject = store(None);
        assert_eq!(subject.load().unwrap(), None);
        assert_eq!(subject.status().unwrap(), ApiKeyStatus::NotSet);

        subject.save(&session().to_json().unwrap()).unwrap();
        assert_eq!(subject.load().unwrap(), Some(session()));
        assert_eq!(subject.status().unwrap(), ApiKeyStatus::Saved);

        subject.delete().unwrap();
        assert_eq!(subject.load().unwrap(), None);
    }

    #[test]
    fn a_stored_document_that_is_not_a_session_reports_malformed() {
        let subject = store(Some("not json"));
        assert!(matches!(
            subject.load(),
            Err(WorkBuddySessionError::Malformed)
        ));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy::session`
Expected: FAIL — the module and types do not exist.

- [ ] **Step 3: Implement the module**

```rust
use serde::{Deserialize, Serialize};

use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

/// WorkBuddy 会话：Quota01 自己通过设备码登录取得的凭据。
///
/// 只存 Quota01 拥有的凭据。WorkBuddy 登录文件里的加密信封永远不会成为凭据，
/// 因此这里不需要任何只读/借用的概念。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkBuddySession {
    #[serde(default)]
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enterprise_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_expires_at: Option<i64>,
}

fn default_token_type() -> String {
    "Bearer".to_owned()
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkBuddySessionError {
    #[error("WorkBuddy session could not be read or updated.")]
    Storage,
    #[error("WorkBuddy session data is invalid.")]
    Malformed,
}

impl WorkBuddySession {
    pub fn to_json(&self) -> Result<String, WorkBuddySessionError> {
        serde_json::to_string(self).map_err(|_| WorkBuddySessionError::Malformed)
    }

    pub fn from_json(text: &str) -> Result<Self, WorkBuddySessionError> {
        let session: Self =
            serde_json::from_str(text).map_err(|_| WorkBuddySessionError::Malformed)?;
        if session.access_token.trim().is_empty() {
            return Err(WorkBuddySessionError::Malformed);
        }
        Ok(session)
    }

    /// 无 expiresAt 时视为未过期：缺少过期信息不该让一份可用凭据被丢弃。
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_at.is_some_and(|expires| expires <= now_ms)
    }
}

const VAULT_ACCOUNT: &str = "workbuddy-cn-session";

#[derive(Clone)]
pub struct WorkBuddySessionStore {
    store: ApiKeyStore,
}

impl WorkBuddySessionStore {
    pub fn new() -> Self {
        Self {
            store: ApiKeyStore::new_with_sources(VAULT_ACCOUNT, &[], &[]),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(store: ApiKeyStore) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Option<WorkBuddySession>, WorkBuddySessionError> {
        match self.store.load().map_err(|_| WorkBuddySessionError::Storage)? {
            Some(secret) => Ok(Some(WorkBuddySession::from_json(secret.as_str())?)),
            None => Ok(None),
        }
    }

    pub fn save(&self, value: &str) -> Result<(), WorkBuddySessionError> {
        WorkBuddySession::from_json(value)?;
        self.store
            .save(value)
            .map_err(|_| WorkBuddySessionError::Storage)
    }

    pub fn delete(&self) -> Result<(), WorkBuddySessionError> {
        self.store
            .delete()
            .map_err(|_| WorkBuddySessionError::Storage)
    }

    pub fn status(&self) -> Result<ApiKeyStatus, WorkBuddySessionError> {
        self.store
            .status()
            .map_err(|_| WorkBuddySessionError::Storage)
    }
}

impl Default for WorkBuddySessionStore {
    fn default() -> Self {
        Self::new()
    }
}
```

`SecretString` is only needed if the test module references it; drop the import if `cargo clippy` reports it unused.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy::session`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all
git add src-tauri/src/providers/workbuddy/session.rs src-tauri/src/providers/workbuddy/mod.rs
git commit -m "feat(workbuddy): add the Quota01-owned session store"
```

---

### Task 3: The device-code login state machine

**Files:**

- Create: `src-tauri/src/providers/workbuddy/login.rs`
- Create: `src-tauri/src/providers/workbuddy/login_tests.rs`
- Modify: `src-tauri/src/providers/workbuddy/mod.rs` (add `mod login;` and `#[cfg(test)] mod login_tests;`)

**Interfaces:**

- Consumes: `WorkBuddySession` from Task 2, `WorkBuddyClient` from the existing `client.rs`, `crate::providers::test_http`.
- Produces:
  - `pub struct LoginChallenge { pub login_id: String, pub verification_uri: String, pub expires_in: u64 }`
  - `pub enum LoginPoll { Pending, Ready(Box<WorkBuddySession>), Failed(String) }`
  - `pub struct DeviceCodeLogin` with `new(base_url: &str, client: Arc<WorkBuddyClient>) -> Self`, `start(&self) -> Result<LoginChallenge, WorkBuddyLoginError>`, `poll(&self, login_id: &str) -> LoginPoll`, `cancel(&self, login_id: &str) -> bool`, `for_test(base_url: &str) -> Self`
  - `pub enum WorkBuddyLoginError { Connection, InvalidResponse }` — note that `poll` reports a rejected login through `LoginPoll::Failed`, not through this error, because polling continues after a per-attempt failure.

- [ ] **Step 1: Write the failing tests**

Create `login_tests.rs`:

```rust
use serde_json::json;

use super::login::{DeviceCodeLogin, LoginPoll, WorkBuddyLoginError, LOGIN_TTL_SECONDS};
use crate::providers::test_http;

#[test]
fn start_returns_the_state_and_authorization_url() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 0, "data": {"state": "st-1", "authUrl": "https://example.test/auth?state=st-1"}})
            .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);

    let challenge = login.start().unwrap();

    assert_eq!(challenge.verification_uri, "https://example.test/auth?state=st-1");
    assert_eq!(challenge.expires_in, LOGIN_TTL_SECONDS);
    assert!(!challenge.login_id.is_empty());
}

#[test]
fn start_without_a_state_is_an_invalid_response() {
    let server = test_http::serve_once(200, &[], &json!({"code": 0, "data": {}}).to_string());
    let login = DeviceCodeLogin::for_test(&server);

    assert!(matches!(
        login.start(),
        Err(WorkBuddyLoginError::InvalidResponse)
    ));
}

#[test]
fn polling_before_authorization_stays_pending() {
    let server = test_http::serve_once(200, &[], &json!({"code": 12153, "msg": "pending"}).to_string());
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.start_for_test();

    assert!(matches!(login.poll(&login_id), LoginPoll::Pending));
}

#[test]
fn polling_after_authorization_returns_the_session() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({
            "code": 0,
            "data": {
                "accessToken": "access-1",
                "refreshToken": "refresh-1",
                "tokenType": "Bearer",
                "domain": "www.codebuddy.cn",
                "expiresAt": 1_800_000_000_000i64,
                "refreshExpiresAt": 1_800_600_000_000i64
            }
        })
        .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.start_for_test();

    match login.poll(&login_id) {
        LoginPoll::Ready(session) => {
            assert_eq!(session.access_token, "access-1");
            assert_eq!(session.refresh_token.as_deref(), Some("refresh-1"));
            assert_eq!(session.domain, "www.codebuddy.cn");
            assert_eq!(session.expires_at, Some(1_800_000_000_000));
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn a_domain_outside_the_workbuddy_family_is_rejected() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({
            "code": 0,
            "data": {"accessToken": "access-1", "domain": "evil.example"}
        })
        .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.start_for_test();

    match login.poll(&login_id) {
        LoginPoll::Failed(message) => assert!(message.contains("evil.example")),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn polling_an_unknown_login_id_fails_without_a_request() {
    let login = DeviceCodeLogin::for_test("http://127.0.0.1:1");
    assert!(matches!(login.poll("missing"), LoginPoll::Failed(_)));
    assert!(!login.cancel("missing"));
}

#[test]
fn a_cancelled_login_stops_polling() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 0, "data": {"state": "st-1", "authUrl": "https://example.test/a"}})
            .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.start_for_test();

    assert!(login.cancel(&login_id));
    assert!(
        matches!(login.poll(&login_id), LoginPoll::Failed(_)),
        "a cancelled attempt must not keep polling"
    );
    assert!(!login.cancel(&login_id));
}
```

Add the test-only helper to `login.rs`. It returns the generated login id, which is what `poll` and `cancel` take:

```rust
    #[cfg(test)]
    pub(crate) fn start_for_test(&self) -> String {
        self.start().unwrap().login_id
    }
```

A login id keys an internal map holding the server-issued `state`, its expiry, and a cancelled flag; `poll` and `cancel` read that map, so no test needs to reach into login internals.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy::login`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Implement `login.rs`**

Key behaviors the implementation must have:

- `LOGIN_TTL_SECONDS: u64 = 600`.
- `start()` posts JSON `{}` to `{base}/v2/plugin/auth/state?platform=workbuddy`, reads `data.state` and `data.authUrl` (also accepting `auth_url` and `url`), generates a login id, records `{state, expires_at}` in a `Mutex<HashMap<String, PendingLogin>>`, and returns the challenge. A missing or blank `state` is `InvalidResponse`; a transport error is `Connection`.
- `poll(login_id)` looks the entry up; an unknown id, a cancelled entry, or one past `expires_at` returns `Failed` with a message and marks the entry done. Otherwise it gets `{base}/v2/plugin/auth/token?state={state}`. `code` neither `0` nor `200` means `Pending`. A missing or blank `accessToken` means `Pending`. A `domain` that is not `www.codebuddy.cn`, `codebuddy.cn`, `www.workbuddy.cn`, or `workbuddy.cn` returns `Failed` and is never stored. On success it builds a `WorkBuddySession`, marks the entry done, and returns `Ready`.
- `expires_at` / `refresh_expires_at` accept `expiresAt`/`expires_at` numbers, falling back to `expiresIn`/`refreshExpiresIn` seconds added to the current time.
- `cancel(login_id)` marks the entry done and returns whether it existed.
- The profile fetch (`/v2/plugin/login/account?state=`) is performed by the provider after `Ready`, not by `poll`, so a profile failure cannot discard a working token.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy::login`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all
git add src-tauri/src/providers/workbuddy/login.rs src-tauri/src/providers/workbuddy/login_tests.rs src-tauri/src/providers/workbuddy/mod.rs
git commit -m "feat(workbuddy): add the device-code login state machine"
```

---

### Task 4: Credential precedence and session hooks in the provider

**Files:**

- Modify: `src-tauri/src/providers/workbuddy/mod.rs`
- Test: the `mod tests` block in `mod.rs`

**Interfaces:**

- Consumes: `WorkBuddySession`, `WorkBuddySessionStore` (Task 2); `DeviceCodeLogin` (Task 3).
- Produces: `WorkBuddyProvider::new()`, `WorkBuddyProvider::for_test(base_url: &str)`, and a private `fn load_credentials(&self) -> Result<WorkBuddyCredential, WorkBuddyError>` where

```rust
enum WorkBuddyCredential {
    /// Quota01 自己登录得到的会话：可以刷新并写回。
    Session(WorkBuddySession),
    /// WorkBuddy 老版本留下的明文登录文件：沿用原有的原子写回。
    AuthFile(WorkBuddyAuth),
}
```

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn a_stored_session_wins_over_the_login_file() {
        let provider = WorkBuddyProvider::for_test_with_session(Some("access-session"), None);
        match provider.load_credentials().unwrap() {
            WorkBuddyCredential::Session(session) => {
                assert_eq!(session.access_token, "access-session")
            }
            WorkBuddyCredential::AuthFile(_) => panic!("the stored session must win"),
        }
    }

    #[test]
    fn the_login_file_is_used_when_no_session_is_stored() {
        let provider = WorkBuddyProvider::for_test_with_session(None, Some("access-file"));
        match provider.load_credentials().unwrap() {
            WorkBuddyCredential::AuthFile(auth) => assert_eq!(auth.access_token, "access-file"),
            WorkBuddyCredential::Session(_) => panic!("the login file must be the fallback"),
        }
    }

    #[test]
    fn an_encrypted_login_file_without_a_session_reports_encryption() {
        let provider = WorkBuddyProvider::for_test_with_session(None, Some("ENCRYPTED"));
        assert!(matches!(
            provider.load_credentials(),
            Err(WorkBuddyError::CredentialsEncrypted)
        ));
    }
```

`for_test_with_session(session_token, file_token)` is a test-only constructor that builds the provider with a temporary vault and a temporary login file, treating the sentinel `"ENCRYPTED"` as an envelope document.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy::tests`
Expected: FAIL — the constructor and `load_credentials` do not exist.

- [ ] **Step 3: Implement precedence**

- Add `sessions: WorkBuddySessionStore` to `WorkBuddyProvider`.
- `load_credentials()` tries `self.sessions.load()?` first; when it returns a session whose `is_expired(now_ms)` is false, return `Session`. An expired stored session is still returned so the existing refresh path can renew it; refresh failure then surfaces as `TokenExpired`.
- Otherwise call `WorkBuddyAuth::load()`, mapping `WorkBuddyAuthError::Encrypted` to `WorkBuddyError::CredentialsEncrypted`.
- Both branches convert into the `WorkBuddyAuth`-shaped inputs the client already expects. Keep `request_base_url`, `usage_base_url`, `token_type`, `uid`, and `enterprise_id` semantics identical: the session's `domain` drives host selection the same way `WorkBuddyAuth::domain` does.
- `refresh_with_identity` refreshes whichever source supplied the credential, and persists back to that source only: a `Session` writes through `sessions.save(...)`; an `AuthFile` keeps the existing `save_tokens` behaviour.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::workbuddy`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all
git add src-tauri/src/providers/workbuddy/mod.rs
git commit -m "feat(workbuddy): prefer the Quota01 session and refresh it in place"
```

---

### Task 5: The device-code trait hook and catalog list

**Files:**

- Modify: `src-tauri/src/providers/mod.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/providers/registry.rs`
- Modify: `src-tauri/src/providers/workbuddy/mod.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/metrics.ts`
- Test: `src-tauri/src/providers/registry.rs` tests, `src/lib/metrics.test.ts`

**Interfaces:**

- Consumes: Task 3's `DeviceCodeLogin`.
- Produces:
  - `pub struct DeviceCodeAuth { pub platform: String }` in `providers/mod.rs`
  - trait methods `fn device_code_auth(&self) -> Option<DeviceCodeAuth> { None }`, `fn start_device_code_login(&self) -> Result<DeviceCodeChallenge, ProviderError>`, `fn poll_device_code_login(&self, login_id: &str) -> DeviceCodePoll`, `fn cancel_device_code_login(&self, login_id: &str) -> bool`
  - `ProviderCatalog.device_code_sign_in_provider_ids: Vec<String>`
  - TS: `deviceCodeSignInProviderIds: string[]` on `ProviderCatalog`, and `ProviderCatalogIndex.supportsDeviceCodeSignIn(id: string): boolean`

- [ ] **Step 1: Write the failing tests**

In `registry.rs`:

```rust
    #[test]
    fn registry_exposes_device_code_sign_in_capabilities() {
        let registry = ProviderRegistry::from_definitions(vec![
            Arc::new(DeviceCodeStubProvider(definition("device-code"))),
            Arc::new(ProviderStubProvider(definition("plain"))),
        ])
        .unwrap();

        assert_eq!(
            registry.catalog().device_code_sign_in_provider_ids,
            ["device-code"]
        );
    }
```

In `src/lib/metrics.test.ts`:

```ts
it('reports device-code sign-in providers from the catalog', () => {
  const catalog = new ProviderCatalogIndex({
    providers: [],
    apiKeyProviderIds: [],
    webviewAuthProviderIds: [],
    deviceCodeSignInProviderIds: ['workbuddy-cn'],
  });

  expect(catalog.supportsDeviceCodeSignIn('workbuddy-cn')).toBe(true);
  expect(catalog.supportsDeviceCodeSignIn('codex')).toBe(false);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers::registry` and `corepack pnpm test -- src/lib/metrics.test.ts`
Expected: FAIL — the field and helper do not exist.

- [ ] **Step 3: Implement the hook, the catalog field, and the TS mirror**

- `models.rs`: add `device_code_sign_in_provider_ids: Vec<String>` to `ProviderCatalog`, marked `#[serde(default)]`.
- `types.ts`: add `deviceCodeSignInProviderIds?: string[]` to the `ProviderCatalog` interface.
- `registry.rs`: collect ids where `provider.device_code_auth().is_some()` and place them in the catalog.
- `metrics.ts`: store the set and add `supportsDeviceCodeSignIn`.
- `providers/mod.rs`: declare `DeviceCodeAuth` plus the four trait methods with inert defaults. The default `start_device_code_login` returns `ProviderError::new(ProviderErrorKind::Internal, "That provider does not use a device-code sign-in.")`; the default `poll_device_code_login` returns `DeviceCodePoll::Failed(...)`; the default `cancel_device_code_sign_in` is `false`.
- `workbuddy/mod.rs`: implement `device_code_auth()` returning `Some(DeviceCodeAuth { platform: "workbuddy".into() })`, and forward the three operations to a `DeviceCodeLogin` held by the provider. On `Ready`, fetch the profile from `/v2/plugin/login/account?state=` with `Authorization: Bearer`, then save the session and return `Ready` so the UI can refresh.

- [ ] **Step 4: Run the tests and the contract scripts**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib providers` then `corepack pnpm verify:contracts && corepack pnpm test -- src/lib/metrics.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all
git add src-tauri/src/providers/mod.rs src-tauri/src/models.rs src-tauri/src/providers/registry.rs src-tauri/src/providers/workbuddy/mod.rs src/lib/types.ts src/lib/metrics.ts
git commit -m "feat(providers): expose device-code sign-in capabilities"
```

---

### Task 6: The three Tauri commands

**Files:**

- Create: `src-tauri/src/commands/provider_login.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/backend.ts`
- Test: `src-tauri/src/commands/provider_login.rs`

**Interfaces:**

- Consumes: the Task 5 trait methods.
- Produces:
  - `start_provider_login(registry, provider_id) -> Result<DeviceCodeChallenge, String>`
  - `poll_provider_login(registry, provider_id, login_id) -> Result<DeviceCodePoll, String>`
  - `cancel_provider_login(registry, provider_id, login_id) -> Result<bool, String>`
  - `backend.ts`: `startProviderLogin`, `pollProviderLogin`, `cancelProviderLogin`

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn an_unknown_provider_cannot_start_a_login() {
        let registry = Arc::new(registry_with(vec![stub("plain")]));
        assert!(start_provider_login(registry, "missing".into()).is_err());
    }

    #[test]
    fn a_provider_without_device_code_sign_in_cannot_start_a_login() {
        let registry = Arc::new(registry_with(vec![stub("plain")]));
        let error = start_provider_login(registry, "plain".into()).unwrap_err();
        assert!(error.contains("device-code"), "unexpected message: {error}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib commands::provider_login`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Implement the commands and register them**

- Follow the shape of `commands/provider.rs`: resolve `registry.runtime(&provider_id)`, returning `"Unknown provider."` when absent, then delegate and map errors with `to_string()`.
- Add `pub mod provider_login;` to `commands/mod.rs`.
- Add the three names to the `tauri::generate_handler![…]` block in `lib.rs`.
- Add the three `invoke` wrappers to `backend.ts`, mirroring the existing session wrappers:

```ts
export function startProviderLogin(providerId: string) {
  return invoke<DeviceCodeChallenge>('start_provider_login', { providerId });
}

export function pollProviderLogin(providerId: string, loginId: string) {
  return invoke<DeviceCodePoll>('poll_provider_login', { providerId, loginId });
}

export function cancelProviderLogin(providerId: string, loginId: string) {
  return invoke<boolean>('cancel_provider_login', { providerId, loginId });
}
```

- Add `DeviceCodeChallenge` and `DeviceCodePoll` interfaces to `types.ts` matching the Rust fields (`loginId`, `verificationUri`, `expiresIn`; `done`, `session`, `error`), serialized `camelCase`.

- [ ] **Step 4: Run the tests and the command contract**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib commands::provider_login` then `corepack pnpm verify:contracts`
Expected: PASS, and the contract script reports the new frontend commands as registered.

- [ ] **Step 5: Commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all
git add src-tauri/src/commands/provider_login.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/backend.ts src/lib/types.ts
git commit -m "feat(commands): add device-code sign-in commands"
```

---

### Task 7: The sign-in panel and its wiring

**Files:**

- Create: `src/lib/ProviderDeviceCodeLogin.svelte`
- Create: `src/lib/ProviderDeviceCodeLogin.test.ts`
- Modify: `src/lib/CustomizeProviderDetail.svelte`
- Modify: `src/lib/Dashboard.svelte`

**Interfaces:**

- Consumes: Task 6's `startProviderLogin`, `pollProviderLogin`, `cancelProviderLogin`; the existing `getProviderSessionState`, `deleteProviderSession`; `catalog.supportsDeviceCodeSignIn`.
- Produces: the `ProviderDeviceCodeLogin` component with props `{ providerId: string; providerName: string; compact?: boolean }`.

- [ ] **Step 1: Write the failing tests**

```ts
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

vi.mock('./backend', () => ({
  getProviderSessionState: vi.fn().mockResolvedValue(null),
  deleteProviderSession: vi.fn().mockResolvedValue(null),
  startProviderLogin: vi.fn().mockResolvedValue({
    loginId: 'l-1',
    verificationUri: 'https://example.test/auth',
    expiresIn: 600,
  }),
  pollProviderLogin: vi.fn().mockResolvedValue({ done: false, session: null, error: null }),
  cancelProviderLogin: vi.fn().mockResolvedValue(true),
  openProviderLink: vi.fn(),
}));

describe('ProviderDeviceCodeLogin', () => {
  it('shows the authorization link after starting a sign-in', async () => {
    render(ProviderDeviceCodeLogin, { providerId: 'workbuddy-cn', providerName: 'Workbuddy CN' });
    await fireEvent.click(screen.getByRole('button', { name: /providerSession\.startSignIn/ }));
    await waitFor(() => expect(screen.getByText('https://example.test/auth')).toBeTruthy());
  });

  it('reports a completion once polling returns a session', async () => {
    const backend = await import('./backend');
    vi.mocked(backend.pollProviderLogin).mockResolvedValue({
      done: true,
      session: null,
      error: null,
    });
    render(ProviderDeviceCodeLogin, { providerId: 'workbuddy-cn', providerName: 'Workbuddy CN' });
    await fireEvent.click(screen.getByRole('button', { name: /providerSession\.startSignIn/ }));
    await waitFor(() => expect(screen.getByRole('status').textContent).toContain('connected'));
  });
});
```

Match the assertion style already used by `ProviderSessionSection.test.ts` in this repository rather than inventing a new one.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `corepack pnpm test -- src/lib/ProviderDeviceCodeLogin.test.ts`
Expected: FAIL — the component does not exist.

- [ ] **Step 3: Implement the component**

- Reuse `ProviderSessionActions.svelte`'s structure and CSS classes for the header, status line, and action buttons so the two panels look identical.
- States: idle (a "start sign-in" button), authorizing (the verification URI in selectable text plus an "open in browser" button calling `openProviderLink`-style opening, a "cancel" button, and a polling loop on a timer), error, and connected (reusing the existing disconnect that calls `deleteProviderSession`).
- Poll on an interval no faster than once per two seconds, stop on `done`, on cancel, and on unmount, and surface a distinct message when the server reports the attempt expired.
- Reuse the existing `providerSession.*` keys where the wording already fits and add only the genuinely new keys.

- [ ] **Step 4: Wire the two render sites**

- `CustomizeProviderDetail.svelte`: insert a branch before the `{#if catalog.supportsWebviewAuth(provider.id)}` at line 444 that renders `ProviderSessionSection`-style markup around `ProviderDeviceCodeLogin` when `catalog.supportsDeviceCodeSignIn(provider.id)`.
- `Dashboard.svelte`: in the error-row action block at line 574, render the device-code panel when `catalog.supportsDeviceCodeSignIn(provider.id)` and the error kind is `authentication` or `credentialStorage`.

- [ ] **Step 5: Run the tests**

Run: `corepack pnpm test -- src/lib/ProviderDeviceCodeLogin.test.ts src/lib/CustomizeProviderDetail.session.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lib/ProviderDeviceCodeLogin.svelte src/lib/ProviderDeviceCodeLogin.test.ts src/lib/CustomizeProviderDetail.svelte src/lib/Dashboard.svelte
git commit -m "feat(ui): add the device-code sign-in panel"
```

---

### Task 8: Localization

**Files:**

- Modify: `src/lib/i18n/messages/*.ts` (16 files)
- Modify: `src/lib/i18n/backendGlossary.ts`
- Test: `src/lib/i18n.test.ts`

**Interfaces:**

- Consumes: the strings introduced in Tasks 4, 6, and 7.
- Produces: `providerSession.startSignIn`, `providerSession.authorizing`, `providerSession.openInBrowser`, `providerSession.cancelSignIn`, `providerSession.signInExpired`, `providerSession.verificationUriLabel`, and any backend error strings added in Task 4.

- [ ] **Step 1: Add the keys to `en.ts`**

```ts
    startSignIn: 'Sign In',
    authorizing: 'Waiting for authorization…',
    verificationUriLabel: 'Open this link in your browser',
    openInBrowser: 'Open in Browser',
    cancelSignIn: 'Cancel',
    signInExpired: 'The sign-in attempt expired. Start again.',
```

- [ ] **Step 2: Mirror the keys into the other 15 locale files**

Add every key to `ar`, `de`, `es`, `fr`, `hi`, `it`, `ja`, `ko`, `pl`, `pt-BR`, `ru`, `tr`, `vi`, `zh-CN`, `zh-TW`. Provide real translations for `zh-CN` and `zh-TW`; use the English text elsewhere, matching the existing convention in those files.

- [ ] **Step 3: Map the backend strings**

Add a `backendGlossary.ts` entry for each new backend message produced in Task 4, mapping the exact Rust string to its `providerError.*` key. Cross-check each mapping by comparing the Rust `#[error("…")]` text with the glossary key character for character.

- [ ] **Step 4: Run the tests**

Run: `corepack pnpm test -- src/lib/i18n.test.ts && corepack pnpm lint`
Expected: PASS, and Prettier reports no issues.

- [ ] **Step 5: Commit**

```bash
git add src/lib/i18n/messages src/lib/i18n/backendGlossary.ts
git commit -m "feat(i18n): localize the WorkBuddy sign-in"
```

---

### Task 9: Documentation and the live acceptance run

**Files:**

- Modify: `docs/providers/workbuddy-cn.md`

- [ ] **Step 1: Rewrite the setup and troubleshooting sections**

Describe: sign in to WorkBuddy from Quota01, authorization happens in the browser, the session lives in Quota01's own encrypted vault, refresh is written back by Quota01, and the legacy plaintext login file is still read as a fallback. Replace the WorkBuddy-5.6 section that currently documents the `workbuddy-switch` borrow, since that dependency is gone. Keep a troubleshooting entry for encrypted credentials that tells the user to sign in from Quota01.

- [ ] **Step 2: Run the full gate**

```bash
corepack pnpm verify
```

Expected: versions, contracts, frontend lint/check/test/build, and Rust fmt/clippy/test all pass.

- [ ] **Step 3: Run the live acceptance test with the user**

```bash
corepack pnpm tauri dev
```

Then, with the user present: open the WorkBuddy card, start the sign-in, open the authorization link in the browser, confirm, and verify `~/Library/Logs/Quota01/Quota01.log` reports `[plugin:workbuddy-cn] refresh end` with a real duration and no warnings, and that the card shows credits. This is the only step that can prove the polling half of the device-code flow.

- [ ] **Step 4: Commit**

```bash
git add docs/providers/workbuddy-cn.md
git commit -m "docs(workbuddy): document the built-in sign-in"
```

---

## Self-Review

**Spec coverage**

- Credential precedence (session → plaintext file → actionable error) — Tasks 2, 4.
- Device-code flow and its endpoint usage — Task 3.
- Provider-local login rather than a shared abstraction — Tasks 3 and 5 keep the flow in the provider and expose only a narrow trait hook.
- Vault storage with `uid` retained for future multi-account — Task 2.
- Commands dispatched by `provider_id` — Task 6.
- Frontend affordance plus its two render sites — Task 7.
- Localization across all locales and the backend glossary — Task 8.
- Error states table — Tasks 3, 4, and 8 (each error kind is asserted by a task-level test).
- Removal of the interim borrow — Task 1.
- `has_local_credentials` revision retained — Task 1 keeps `an_encrypted_login_file_still_counts_as_local_credentials`.
- Non-goals (no decryption, no other app's store, single account, no AI variant) — no task implements them, by construction.
- Live acceptance requirement — Task 9 Step 3.
- Documentation update — Task 9.

**Placeholder scan**

No `TBD`/`TODO`/"handle edge cases" steps remain. Every code step carries real code or a precise behavioural specification with the exact endpoints, field names, and error variants.

**Type consistency**

- `WorkBuddySession` fields are used identically in Tasks 2, 3, and 4.
- `DeviceCodeChallenge`/`DeviceCodePoll` are named the same in `models.rs` (Task 5/6) and `types.ts` (Task 6).
- `device_code_sign_in_provider_ids` (Rust) maps to `deviceCodeSignInProviderIds` (TS) per the model-contract camelCase rule.
- `supportsDeviceCodeSignIn` is spelled the same in `metrics.ts`, the component tests, and both render sites.

**Issues found while self-reviewing this plan, and fixed inline**

1. Task 2 originally gave the session store a `for_test()` constructor that reused the real `ApiKeyStore` defaults, which would have written test data into the user's actual encrypted credential vault. `MemorySecrets` already exists but is private to `api_key.rs`'s test module, so the plan now follows the DeepSeek and Trae precedent: an in-test `MemorySecrets`/`EmptyEnvironment` pair plus a `#[cfg(test)] with_store(ApiKeyStore)` constructor, mirroring `DeepSeekAuthStore::with_store`.
2. Task 3 originally used an undefined `lookup_state` helper, and its cancel test called `start()` against an unreachable port and unwrapped it, which would panic. The helper now returns the generated login id, and the cancel test starts against a stub server.
3. `WorkBuddyLoginError::DomainMismatch` was removed from the interface: `poll` reports a rejected login through `LoginPoll::Failed` so the UI keeps its polling semantics, which left that variant unused.
