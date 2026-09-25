use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde_json::Value;
use tempfile::NamedTempFile;

use super::CodexError;

const REFRESH_WINDOW: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct CodexAuthState {
    source: AuthSource,
    pub document: Value,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub last_refresh: Option<String>,
}

#[derive(Debug, Clone)]
enum AuthSource {
    File(PathBuf),
}

impl CodexAuthState {
    pub fn has_local_credentials() -> bool {
        #[cfg(target_os = "macos")]
        {
            return Self::load_candidates_from_paths(&auth_paths()).is_ok();
        }
        #[cfg(not(target_os = "macos"))]
        Self::has_file_credentials_from_paths(&auth_paths())
    }

    fn has_file_credentials_from_paths(paths: &[PathBuf]) -> bool {
        !load_file_candidates_from_paths(paths).0.is_empty()
    }

    pub fn load_candidates() -> Result<Vec<Self>, CodexError> {
        Self::load_candidates_from_paths(&auth_paths())
    }

    pub(super) fn load_candidates_from_paths(paths: &[PathBuf]) -> Result<Vec<Self>, CodexError> {
        #[cfg(target_os = "macos")]
        {
            return load_macos_file_candidates_from_paths(paths);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let (candidates, api_key_only) = load_file_candidates_from_paths(paths);
            if !candidates.is_empty() {
                Ok(candidates)
            } else if api_key_only {
                Err(CodexError::ApiKeyOnly)
            } else {
                Err(CodexError::NotLoggedIn)
            }
        }
    }

    fn load_file_candidates() -> (Vec<Self>, bool) {
        load_file_candidates_from_paths(&auth_paths())
    }

    pub fn observed_account_identity() -> Option<String> {
        Self::load_file_candidates()
            .0
            .into_iter()
            .find_map(|state| state.account_identity())
    }

    pub(super) fn account_identity(&self) -> Option<String> {
        self.account_id
            .as_deref()
            .and_then(nonempty_lowercase)
            .or_else(|| {
                self.document
                    .pointer("/tokens/id_token")
                    .and_then(Value::as_str)
                    .and_then(jwt_payload)
                    .and_then(|payload| {
                        payload
                            .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
                            .or_else(|| payload.get("chatgpt_account_id"))
                            .and_then(Value::as_str)
                            .and_then(nonempty_lowercase)
                    })
            })
    }

    pub fn reload(&self) -> Result<Self, CodexError> {
        match &self.source {
            AuthSource::File(path) => load_from_path(path),
        }
    }

    pub fn needs_refresh(&self, now: DateTime<Utc>) -> bool {
        if let Some(expiry) = jwt_expiry(&self.access_token) {
            return expiry.signed_duration_since(now).num_seconds()
                <= REFRESH_WINDOW.as_secs() as i64;
        }
        self.last_refresh
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|date| now.signed_duration_since(date.to_utc()).num_days() > 8)
    }

    pub(super) fn update_and_save_if_current(
        &mut self,
        access_token: String,
        refresh_token: Option<String>,
        id_token: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), CodexError> {
        let current = self.reload().map_err(|_| CodexError::AccountChanged)?;
        if current.document != self.document {
            return Err(CodexError::AccountChanged);
        }
        self.update_and_save(access_token, refresh_token, id_token, now)
    }

    fn update_and_save(
        &mut self,
        access_token: String,
        refresh_token: Option<String>,
        id_token: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), CodexError> {
        set_string(&mut self.document, "/tokens/access_token", &access_token)?;
        if let Some(value) = refresh_token.as_deref() {
            set_string(&mut self.document, "/tokens/refresh_token", value)?;
            self.refresh_token = Some(value.to_owned());
        }
        if let Some(value) = id_token.as_deref() {
            set_string(&mut self.document, "/tokens/id_token", value)?;
        }
        let refreshed_at = now.to_rfc3339();
        set_string(&mut self.document, "/last_refresh", &refreshed_at)?;
        self.access_token = access_token;
        self.last_refresh = Some(refreshed_at);

        match &self.source {
            AuthSource::File(path) => save_file_document(path, &self.document),
        }
    }
}

fn load_file_candidates_from_paths(paths: &[PathBuf]) -> (Vec<CodexAuthState>, bool) {
    let mut candidates = Vec::new();
    let mut api_key_only = false;
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Some(document) = parse_auth_document(&text) else {
            continue;
        };
        let access_token = document
            .pointer("/tokens/access_token")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(access_token) = access_token {
            candidates.push(CodexAuthState {
                source: AuthSource::File(path.clone()),
                refresh_token: string_at(&document, "/tokens/refresh_token"),
                account_id: string_at(&document, "/tokens/account_id"),
                last_refresh: string_at(&document, "/last_refresh"),
                document,
                access_token,
            });
            continue;
        }
        api_key_only |= document
            .get("OPENAI_API_KEY")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty());
    }
    (candidates, api_key_only)
}

#[cfg(target_os = "macos")]
fn load_macos_file_candidates_from_paths(
    paths: &[PathBuf],
) -> Result<Vec<CodexAuthState>, CodexError> {
    let mut candidates = Vec::new();
    let mut api_key_only = false;
    let mut unreadable = false;
    let mut malformed = false;
    for path in paths {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                unreadable = true;
                continue;
            }
        };
        let Some(document) = parse_auth_document(&text) else {
            malformed = true;
            continue;
        };
        let access_token = document
            .pointer("/tokens/access_token")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(access_token) = access_token {
            candidates.push(CodexAuthState {
                source: AuthSource::File(path.clone()),
                refresh_token: string_at(&document, "/tokens/refresh_token"),
                account_id: string_at(&document, "/tokens/account_id"),
                last_refresh: string_at(&document, "/last_refresh"),
                document,
                access_token,
            });
        } else {
            api_key_only |= document
                .get("OPENAI_API_KEY")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
        }
    }
    if !candidates.is_empty() {
        return Ok(candidates);
    }
    if api_key_only {
        return Err(CodexError::ApiKeyOnly);
    }
    if malformed {
        return Err(CodexError::InvalidAuth);
    }
    if unreadable {
        return Err(CodexError::CredentialRead);
    }
    Err(CodexError::NotLoggedIn)
}

fn load_from_path(path: &Path) -> Result<CodexAuthState, CodexError> {
    let text = fs::read_to_string(path).map_err(|_| CodexError::InvalidAuth)?;
    let document = parse_auth_document(&text).ok_or(CodexError::InvalidAuth)?;
    let access_token = string_at(&document, "/tokens/access_token")
        .filter(|value| !value.is_empty())
        .ok_or(CodexError::NotLoggedIn)?;
    Ok(CodexAuthState {
        source: AuthSource::File(path.to_path_buf()),
        refresh_token: string_at(&document, "/tokens/refresh_token"),
        account_id: string_at(&document, "/tokens/account_id"),
        last_refresh: string_at(&document, "/last_refresh"),
        document,
        access_token,
    })
}

fn save_file_document(path: &Path, document: &Value) -> Result<(), CodexError> {
    let parent = path.parent().ok_or(CodexError::InvalidAuth)?;
    let mut temporary = NamedTempFile::new_in(parent).map_err(|_| CodexError::AuthWrite)?;
    serde_json::to_writer_pretty(&mut temporary, document).map_err(|_| CodexError::AuthWrite)?;
    temporary
        .write_all(b"\n")
        .map_err(|_| CodexError::AuthWrite)?;
    temporary.flush().map_err(|_| CodexError::AuthWrite)?;
    temporary.persist(path).map_err(|_| CodexError::AuthWrite)?;
    Ok(())
}

pub fn auth_paths() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    candidate_paths(
        &home,
        crate::provider_environment::value("CODEX_HOME")
            .map(PathBuf::from)
            .as_deref(),
    )
}

fn candidate_paths(home: &Path, codex_home: Option<&Path>) -> Vec<PathBuf> {
    if let Some(codex_home) = codex_home.filter(|path| !path.as_os_str().is_empty()) {
        return vec![codex_home.join("auth.json")];
    }
    vec![
        home.join(".config").join("codex").join("auth.json"),
        home.join(".codex").join("auth.json"),
    ]
}

fn parse_auth_document(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok().or_else(|| {
        let trimmed = text.trim();
        if !trimmed.len().is_multiple_of(2) || !trimmed.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let bytes = (0..trimmed.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&trimmed[index..index + 2], 16).ok())
            .collect::<Option<Vec<_>>>()?;
        serde_json::from_slice(&bytes).ok()
    })
}

fn jwt_expiry(token: &str) -> Option<DateTime<Utc>> {
    let value = jwt_payload(token)?;
    DateTime::from_timestamp(value.get("exp")?.as_i64()?, 0)
}

fn jwt_payload(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn nonempty_lowercase(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_ascii_lowercase())
}

fn string_at(document: &Value, pointer: &str) -> Option<String> {
    document
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
fn auth_document_has_credentials(document: &Value) -> bool {
    document
        .pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

fn set_string(document: &mut Value, pointer: &str, value: &str) -> Result<(), CodexError> {
    let segments = pointer
        .split('/')
        .skip(1)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let (leaf, parents) = segments.split_last().ok_or(CodexError::InvalidAuth)?;
    let mut cursor = document;
    for segment in parents {
        let object = cursor.as_object_mut().ok_or(CodexError::InvalidAuth)?;
        cursor = object
            .entry((*segment).to_owned())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    cursor
        .as_object_mut()
        .ok_or(CodexError::InvalidAuth)?
        .insert((*leaf).to_owned(), Value::String(value.to_owned()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        auth_document_has_credentials, candidate_paths, parse_auth_document, AuthSource,
        CodexAuthState,
    };
    use crate::providers::codex::CodexError;

    #[test]
    fn codex_home_replaces_default_candidates() {
        assert_eq!(
            candidate_paths(Path::new("/users/me"), Some(Path::new("/custom/codex"))),
            vec![Path::new("/custom/codex/auth.json")]
        );
    }

    #[test]
    fn file_candidate_loading_uses_only_the_supplied_auth_json_files() {
        let directory = tempdir().unwrap();
        let auth_path = directory.path().join("auth.json");
        fs::write(
            &auth_path,
            serde_json::to_vec(&json!({
                "tokens": {"access_token": "file-access", "refresh_token": "file-refresh"}
            }))
            .unwrap(),
        )
        .unwrap();

        let candidates = CodexAuthState::load_candidates_from_paths(&[auth_path.clone()]).unwrap();

        assert_eq!(candidates.len(), 1);
        assert!(
            matches!(&candidates[0].source, AuthSource::File(path) if path.as_path() == auth_path.as_path())
        );
        match &candidates[0].source {
            AuthSource::File(_) => {}
        }
        assert_eq!(candidates[0].access_token, "file-access");
    }

    #[test]
    fn no_auth_files_means_no_file_candidate() {
        assert!(matches!(
            CodexAuthState::load_candidates_from_paths(&[]),
            Err(CodexError::NotLoggedIn)
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_auth_loader_distinguishes_missing_unreadable_and_malformed_files() {
        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing-auth.json");
        let unreadable = directory.path().join("directory-auth.json");
        fs::create_dir(&unreadable).unwrap();
        let malformed = directory.path().join("malformed-auth.json");
        fs::write(&malformed, b"not json").unwrap();

        assert!(matches!(
            CodexAuthState::load_candidates_from_paths(&[missing]),
            Err(CodexError::NotLoggedIn)
        ));
        assert!(matches!(
            CodexAuthState::load_candidates_from_paths(&[unreadable]),
            Err(CodexError::CredentialRead)
        ));
        assert!(matches!(
            CodexAuthState::load_candidates_from_paths(&[malformed]),
            Err(CodexError::InvalidAuth)
        ));
    }

    #[test]
    fn codex_candidate_source_type_is_file_only() {
        let path = PathBuf::from("auth.json");
        let source = AuthSource::File(path);
        match &source {
            AuthSource::File(_) => {}
        }
    }

    #[test]
    fn local_detection_uses_only_file_candidates() {
        let directory = tempdir().unwrap();
        let auth_path = directory.path().join("auth.json");
        fs::write(
            &auth_path,
            serde_json::to_vec(&json!({"tokens": {"access_token": "file-access"}})).unwrap(),
        )
        .unwrap();

        assert!(CodexAuthState::has_file_credentials_from_paths(&[
            auth_path
        ]));
        assert!(!CodexAuthState::has_file_credentials_from_paths(&[]));
    }

    #[test]
    fn refreshed_tokens_write_back_to_the_same_auth_json_and_preserve_other_fields() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("auth.json");
        let original = json!({
            "tokens": {"access_token": "old-access", "refresh_token": "old-refresh"},
            "other_setting": true
        });
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let mut auth = super::load_from_path(&path).unwrap();

        auth.update_and_save(
            "new-access".into(),
            Some("new-refresh".into()),
            None,
            Utc::now(),
        )
        .unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.pointer("/tokens/access_token").unwrap(), "new-access");
        assert_eq!(
            saved.pointer("/tokens/refresh_token").unwrap(),
            "new-refresh"
        );
        assert_eq!(saved.get("other_setting").unwrap(), true);
    }

    #[test]
    fn parses_hex_encoded_auth_without_exposing_tokens() {
        let raw = r#"{"tokens":{"access_token":"placeholder"}}"#;
        let hex = raw
            .bytes()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            parse_auth_document(&hex)
                .unwrap()
                .pointer("/tokens/access_token")
                .and_then(|value| value.as_str()),
            Some("placeholder")
        );
    }

    #[test]
    fn jwt_expiry_controls_refresh_window() {
        let now = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({"exp": (now + Duration::minutes(1)).timestamp()})).unwrap(),
        );
        let state = CodexAuthState {
            source: super::AuthSource::File("auth.json".into()),
            document: json!({}),
            access_token: format!("header.{payload}.signature"),
            refresh_token: None,
            account_id: None,
            last_refresh: None,
        };
        assert!(state.needs_refresh(now));
    }

    #[test]
    fn local_detection_only_accepts_a_usable_access_token() {
        assert!(auth_document_has_credentials(
            &json!({"tokens":{"access_token":"placeholder"}})
        ));
        assert!(!auth_document_has_credentials(
            &json!({"OPENAI_API_KEY":"placeholder"})
        ));
        assert!(!auth_document_has_credentials(
            &json!({"tokens":{"refresh_token":"placeholder"}})
        ));
        assert!(!auth_document_has_credentials(
            &json!({"tokens":{"access_token":""}})
        ));
    }

    #[test]
    fn account_identity_prefers_the_explicit_id_and_falls_back_to_the_id_token() {
        let state = CodexAuthState {
            source: AuthSource::File("auth.json".into()),
            document: json!({}),
            access_token: "access".into(),
            refresh_token: None,
            account_id: Some("  ACCOUNT-A  ".into()),
            last_refresh: None,
        };
        assert_eq!(state.account_identity().as_deref(), Some("account-a"));

        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({
                "https://api.openai.com/auth": {"chatgpt_account_id": "Account-B"}
            }))
            .unwrap(),
        );
        let state = CodexAuthState {
            source: AuthSource::File("auth.json".into()),
            document: json!({"tokens": {"id_token": format!("header.{payload}.signature")}}),
            access_token: "access".into(),
            refresh_token: None,
            account_id: None,
            last_refresh: None,
        };
        assert_eq!(state.account_identity().as_deref(), Some("account-b"));
    }

    #[test]
    fn credential_write_failures_are_typed_and_do_not_expose_tokens() {
        let directory = tempdir().unwrap();
        let blocked_parent = directory.path().join("not-a-directory");
        fs::write(&blocked_parent, b"block directory creation").unwrap();
        let mut state = CodexAuthState {
            source: AuthSource::File(blocked_parent.join("auth.json")),
            document: json!({"tokens": {}}),
            access_token: "old-access".into(),
            refresh_token: Some("old-refresh".into()),
            account_id: None,
            last_refresh: None,
        };

        let error = state
            .update_and_save(
                "secret-access".into(),
                Some("secret-refresh".into()),
                None,
                Utc::now(),
            )
            .unwrap_err();

        assert!(matches!(error, CodexError::AuthWrite));
        assert!(!error.to_string().contains("secret-access"));
        assert!(!error.to_string().contains("secret-refresh"));
    }

    #[test]
    fn refreshed_tokens_do_not_overwrite_credentials_changed_during_refresh() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("auth.json");
        let original = json!({
            "tokens": {
                "access_token": "account-a-access",
                "refresh_token": "account-a-refresh",
                "account_id": "account-a"
            }
        });
        let replacement = json!({
            "tokens": {
                "access_token": "account-b-access",
                "refresh_token": "account-b-refresh",
                "account_id": "account-b"
            }
        });
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let mut state = super::load_from_path(&path).unwrap();
        fs::write(&path, serde_json::to_vec(&replacement).unwrap()).unwrap();

        let error = state
            .update_and_save_if_current(
                "rotated-account-a-access".into(),
                Some("rotated-account-a-refresh".into()),
                None,
                Utc::now(),
            )
            .unwrap_err();

        assert!(matches!(error, CodexError::AccountChanged));
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(persisted, replacement);
    }
}
