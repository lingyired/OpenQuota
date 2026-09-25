use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

#[cfg(not(target_os = "macos"))]
use crate::providers::credential_store::{read_external_password, write_external_password};

use super::CursorError;

const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
const REFRESH_TOKEN_KEY: &str = "cursorAuth/refreshToken";
const MEMBERSHIP_TYPE_KEY: &str = "cursorAuth/stripeMembershipType";
#[cfg(not(target_os = "macos"))]
const ACCESS_TOKEN_SERVICE: &str = "cursor-access-token";
#[cfg(not(target_os = "macos"))]
const REFRESH_TOKEN_SERVICE: &str = "cursor-refresh-token";
const REFRESH_BUFFER_MINUTES: i64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorAuthSource {
    Sqlite(PathBuf),
    #[cfg(not(target_os = "macos"))]
    Keychain {
        account: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorAuthState {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub source: CursorAuthSource,
}

impl CursorAuthState {
    pub fn load() -> Result<Option<Self>, CursorError> {
        let sqlite = sqlite_auth_from_paths(&state_database_paths());
        #[cfg(target_os = "macos")]
        return Ok(sqlite.map(|(state, _)| state));
        #[cfg(not(target_os = "macos"))]
        Ok(select_auth_state(sqlite, load_keychain_auth()))
    }

    pub fn has_local_credentials() -> bool {
        if has_sqlite_auth(&state_database_paths()) {
            return true;
        }
        #[cfg(not(target_os = "macos"))]
        return load_keychain_auth().is_some();
        #[cfg(target_os = "macos")]
        false
    }

    pub fn needs_refresh(&self, now: DateTime<Utc>) -> bool {
        self.access_token
            .as_deref()
            .and_then(token_expiration)
            .is_none_or(|expires| expires - now <= Duration::minutes(REFRESH_BUFFER_MINUTES))
    }

    pub fn save_access_token(&mut self, access_token: String) -> Result<(), CursorError> {
        match &self.source {
            CursorAuthSource::Sqlite(path) => {
                write_state_value(path, ACCESS_TOKEN_KEY, &access_token)
            }
            #[cfg(not(target_os = "macos"))]
            CursorAuthSource::Keychain { account } => write_external_password(
                "cursor",
                ACCESS_TOKEN_SERVICE,
                account,
                access_token.as_bytes(),
            )
            .map_err(|_| CursorError::AuthWrite),
        }?;
        self.access_token = Some(access_token);
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
fn select_auth_state(
    sqlite: Option<(CursorAuthState, Option<String>)>,
    keychain: Option<CursorAuthState>,
) -> Option<CursorAuthState> {
    if let Some((sqlite, membership)) = sqlite {
        let subjects_differ = token_subject(sqlite.access_token.as_deref()).is_some_and(|left| {
            token_subject(
                keychain
                    .as_ref()
                    .and_then(|auth| auth.access_token.as_deref()),
            )
            .is_some_and(|right| right != left)
        });
        if membership.as_deref() == Some("free") && subjects_differ {
            return keychain;
        }
        return Some(sqlite);
    }
    keychain
}

fn sqlite_auth_from_paths(paths: &[PathBuf]) -> Option<(CursorAuthState, Option<String>)> {
    paths.iter().find_map(|path| load_sqlite_auth(path))
}

fn has_sqlite_auth(paths: &[PathBuf]) -> bool {
    sqlite_auth_from_paths(paths).is_some()
}

fn load_sqlite_auth(path: &Path) -> Option<(CursorAuthState, Option<String>)> {
    if !path.is_file() {
        return None;
    }
    let access_token = read_state_value(path, ACCESS_TOKEN_KEY);
    let refresh_token = read_state_value(path, REFRESH_TOKEN_KEY);
    if access_token.is_none() && refresh_token.is_none() {
        return None;
    }
    let membership =
        read_state_value(path, MEMBERSHIP_TYPE_KEY).map(|value| value.to_ascii_lowercase());
    Some((
        CursorAuthState {
            access_token,
            refresh_token,
            source: CursorAuthSource::Sqlite(path.to_path_buf()),
        },
        membership,
    ))
}

fn read_state_value(path: &Path, key: &str) -> Option<String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    connection
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1",
            [key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .and_then(non_empty)
}

fn write_state_value(path: &Path, key: &str, value: &str) -> Result<(), CursorError> {
    let connection = Connection::open(path).map_err(|_| CursorError::AuthWrite)?;
    connection
        .execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
            (key, value),
        )
        .map_err(|_| CursorError::AuthWrite)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn load_keychain_auth() -> Option<CursorAuthState> {
    for account in keychain_accounts() {
        let access_token = read_keychain_value(ACCESS_TOKEN_SERVICE, &account);
        let refresh_token = read_keychain_value(REFRESH_TOKEN_SERVICE, &account);
        if access_token.is_some() || refresh_token.is_some() {
            return Some(CursorAuthState {
                access_token,
                refresh_token,
                source: CursorAuthSource::Keychain { account },
            });
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn read_keychain_value(service: &str, account: &str) -> Option<String> {
    read_external_password("cursor", service, account)
        .ok()
        .flatten()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(non_empty)
}

#[cfg(not(target_os = "macos"))]
fn keychain_accounts() -> Vec<String> {
    let current = std::env::var("USER")
        .ok()
        .or_else(|| std::env::var("USERNAME").ok())
        .and_then(non_empty);
    let mut accounts = vec![String::new()];
    if let Some(current) = current {
        accounts.push(current);
    }
    accounts
}

pub fn token_subject(token: Option<&str>) -> Option<String> {
    jwt_payload(token?)?
        .get("sub")?
        .as_str()
        .and_then(non_empty)
}

fn token_expiration(token: &str) -> Option<DateTime<Utc>> {
    let seconds = jwt_payload(token)?.get("exp").and_then(number)?;
    DateTime::from_timestamp(seconds as i64, 0)
}

fn jwt_payload(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .filter(|value| value.is_finite())
}

fn non_empty(value: impl AsRef<str>) -> Option<String> {
    let value = value.as_ref().trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn state_database_paths() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("QUOTA01_CURSOR_STATE_DB").map(PathBuf::from) {
        return vec![path];
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut paths = Vec::new();
    if let Some(app_data) = std::env::var_os("APPDATA").map(PathBuf::from) {
        paths.push(app_data.join("Cursor/User/globalStorage/state.vscdb"));
    }
    paths.push(home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb"));
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        paths.push(config.join("Cursor/User/globalStorage/state.vscdb"));
    }
    paths.push(home.join(".config/Cursor/User/globalStorage/state.vscdb"));
    paths
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use chrono::{TimeZone, Utc};
    use rusqlite::Connection;
    use tempfile::tempdir;

    use super::*;

    fn jwt(subject: &str, expiration: i64) -> String {
        let payload = serde_json::json!({"sub": subject, "exp": expiration});
        format!("a.{}.c", URL_SAFE_NO_PAD.encode(payload.to_string()))
    }

    fn state(source: CursorAuthSource, access: &str) -> CursorAuthState {
        CursorAuthState {
            access_token: Some(access.into()),
            refresh_token: Some("refresh".into()),
            source,
        }
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn free_sqlite_workspace_prefers_different_agent_subject() {
        let sqlite = state(
            CursorAuthSource::Sqlite("db".into()),
            &jwt("auth0|free", 100),
        );
        let keychain = state(
            CursorAuthSource::Keychain {
                account: String::new(),
            },
            &jwt("auth0|pro", 100),
        );
        let selected = select_auth_state(Some((sqlite, Some("free".into()))), Some(keychain));
        assert!(matches!(
            selected.unwrap().source,
            CursorAuthSource::Keychain { .. }
        ));
    }

    #[test]
    fn detection_and_runtime_candidates_use_only_the_selected_sqlite_database() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.vscdb");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                (ACCESS_TOKEN_KEY, "sqlite-access"),
            )
            .unwrap();
        drop(connection);

        let selected = sqlite_auth_from_paths(std::slice::from_ref(&path))
            .unwrap()
            .0;

        #[cfg(target_os = "macos")]
        match &selected.source {
            CursorAuthSource::Sqlite(_) => {}
        }
        assert!(matches!(selected.source, CursorAuthSource::Sqlite(source) if source == path));
        assert!(!has_sqlite_auth(&[]));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sqlite_is_preferred_for_paid_or_same_subject_sessions() {
        let sqlite = state(
            CursorAuthSource::Sqlite("db".into()),
            &jwt("auth0|paid", 100),
        );
        let keychain = state(
            CursorAuthSource::Keychain {
                account: String::new(),
            },
            &jwt("auth0|other", 100),
        );
        let selected = select_auth_state(Some((sqlite, Some("pro".into()))), Some(keychain));
        assert!(matches!(
            selected.unwrap().source,
            CursorAuthSource::Sqlite(_)
        ));
    }

    #[test]
    fn sqlite_values_are_parameterized_and_refreshed_token_is_saved() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.vscdb");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                (ACCESS_TOKEN_KEY, "old"),
            )
            .unwrap();
        drop(connection);

        let (mut auth, _) = sqlite_auth_from_paths(std::slice::from_ref(&path)).unwrap();
        assert!(matches!(&auth.source, CursorAuthSource::Sqlite(source) if source == &path));
        auth.save_access_token("new'quoted".into()).unwrap();
        assert_eq!(
            read_state_value(&path, ACCESS_TOKEN_KEY).as_deref(),
            Some("new'quoted")
        );
    }

    #[test]
    fn missing_or_near_expiry_jwt_needs_refresh() {
        let now = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap();
        let mut auth = state(CursorAuthSource::Sqlite("db".into()), "invalid");
        assert!(auth.needs_refresh(now));
        auth.access_token = Some(jwt("auth0|user", (now + Duration::minutes(4)).timestamp()));
        assert!(auth.needs_refresh(now));
        auth.access_token = Some(jwt("auth0|user", (now + Duration::minutes(6)).timestamp()));
        assert!(!auth.needs_refresh(now));
    }
}
