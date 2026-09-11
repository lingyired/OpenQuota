use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkBuddyAuthError {
    #[error("WorkBuddy is not logged in.")]
    NotLoggedIn,
    #[error("WorkBuddy login data is invalid.")]
    Invalid,
    #[error("WorkBuddy credentials could not be read or updated.")]
    Storage,
}

#[derive(Debug, Clone)]
pub struct WorkBuddyAuth {
    pub path: PathBuf,
    pub document: Value,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub domain: String,
    pub uid: Option<String>,
    pub enterprise_id: Option<String>,
    // Keep these profile fields available for future account presentation without including
    // them in snapshots, logs, or cache records.
    #[allow(dead_code)]
    pub nickname: Option<String>,
    #[allow(dead_code)]
    pub email: Option<String>,
    #[allow(dead_code)]
    pub expires_at: Option<DateTime<Utc>>,
}

impl WorkBuddyAuth {
    pub fn load() -> Result<Self, WorkBuddyAuthError> {
        Self::load_from_path(&auth_file_path())
    }

    pub fn load_from_path(path: &Path) -> Result<Self, WorkBuddyAuthError> {
        let text = fs::read_to_string(path).map_err(|_| WorkBuddyAuthError::NotLoggedIn)?;
        let document: Value =
            serde_json::from_str(&text).map_err(|_| WorkBuddyAuthError::Invalid)?;
        let access_token = first_string(
            &document,
            &[
                &["auth", "accessToken"],
                &["auth", "access_token"],
                &["accessToken"],
                &["access_token"],
            ],
        )
        .filter(|value| !value.is_empty())
        .ok_or(WorkBuddyAuthError::NotLoggedIn)?;
        Ok(Self {
            path: path.to_owned(),
            refresh_token: first_string(
                &document,
                &[
                    &["auth", "refreshToken"],
                    &["auth", "refresh_token"],
                    &["refreshToken"],
                    &["refresh_token"],
                ],
            ),
            token_type: first_string(
                &document,
                &[
                    &["auth", "tokenType"],
                    &["auth", "token_type"],
                    &["tokenType"],
                    &["token_type"],
                ],
            )
            .unwrap_or_else(|| "Bearer".into()),
            domain: first_string(&document, &[&["domain"], &["auth", "domain"]])
                .unwrap_or_default(),
            uid: first_string(
                &document,
                &[&["uid"], &["account", "uid"], &["account", "id"]],
            ),
            enterprise_id: first_string(
                &document,
                &[
                    &["enterpriseId"],
                    &["enterprise_id"],
                    &["auth", "enterpriseId"],
                    &["auth", "enterprise_id"],
                    &["account", "enterpriseId"],
                    &["account", "enterprise_id"],
                ],
            ),
            nickname: first_string(
                &document,
                &[
                    &["nickname"],
                    &["name"],
                    &["account", "nickname"],
                    &["account", "label"],
                ],
            ),
            email: first_string(
                &document,
                &[&["email"], &["account", "email"], &["auth", "email"]],
            ),
            expires_at: first_value(
                &document,
                &[
                    &["expiresAt"],
                    &["expires_at"],
                    &["auth", "expiresAt"],
                    &["auth", "expires_at"],
                ],
            )
            .and_then(parse_datetime),
            document,
            access_token,
        })
    }

    pub fn has_local_credentials() -> bool {
        Self::load().is_ok()
    }

    pub fn request_base_url(&self) -> &'static str {
        match self.domain.trim().to_ascii_lowercase().as_str() {
            "www.workbuddy.cn" | "workbuddy.cn" => "https://www.workbuddy.cn",
            _ => "https://www.codebuddy.cn",
        }
    }

    pub fn usage_base_url(&self) -> &'static str {
        "https://www.workbuddy.cn"
    }

    pub fn save_tokens(
        &mut self,
        access_token: String,
        refresh_token: Option<String>,
    ) -> Result<(), WorkBuddyAuthError> {
        let current = Self::load_from_path(&self.path)?;
        if current.access_token != self.access_token || current.document != self.document {
            return Err(WorkBuddyAuthError::Storage);
        }
        let object = self
            .document
            .as_object_mut()
            .ok_or(WorkBuddyAuthError::Invalid)?;
        let auth = object
            .entry("auth")
            .or_insert_with(|| Value::Object(Map::new()));
        let auth = auth.as_object_mut().ok_or(WorkBuddyAuthError::Invalid)?;
        auth.insert("accessToken".into(), Value::String(access_token.clone()));
        if let Some(refresh_token) = refresh_token.as_deref() {
            auth.insert(
                "refreshToken".into(),
                Value::String(refresh_token.to_owned()),
            );
        }
        atomic_write(&self.path, &self.document)?;
        self.access_token = access_token;
        if refresh_token.is_some() {
            self.refresh_token = refresh_token;
        }
        Ok(())
    }
}

pub fn auth_file_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info")
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(env::var_os("LOCALAPPDATA").unwrap_or_default())
            .join("CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info");
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home_dir().join(".local/share/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info");
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
}

fn first_value<'a>(root: &'a Value, paths: &[&[&str]]) -> Option<&'a Value> {
    paths
        .iter()
        .find_map(|path| path.iter().try_fold(root, |value, key| value.get(*key)))
}

fn first_string(root: &Value, paths: &[&[&str]]) -> Option<String> {
    first_value(root, paths).and_then(|value| match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn parse_datetime(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
        let millis = if number.abs() < 1.0e10 {
            number * 1000.0
        } else {
            number
        };
        return DateTime::from_timestamp_millis(millis.round() as i64);
    }
    let text = value.as_str()?.trim();
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

fn atomic_write(path: &Path, document: &Value) -> Result<(), WorkBuddyAuthError> {
    let parent = path.parent().ok_or(WorkBuddyAuthError::Storage)?;
    let mut temp = NamedTempFile::new_in(parent).map_err(|_| WorkBuddyAuthError::Storage)?;
    serde_json::to_writer_pretty(&mut temp, document).map_err(|_| WorkBuddyAuthError::Storage)?;
    temp.write_all(b"\n")
        .map_err(|_| WorkBuddyAuthError::Storage)?;
    temp.flush().map_err(|_| WorkBuddyAuthError::Storage)?;
    temp.persist(path)
        .map_err(|_| WorkBuddyAuthError::Storage)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_nested_precedence_and_rejects_missing_access_token() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workbuddy-desktop.info");
        fs::write(&path, r#"{"accessToken":"root","auth":{"accessToken":"nested","refreshToken":"refresh"},"account":{"uid":"u"},"domain":"www.workbuddy.cn","unknown":{"keep":true}}"#).unwrap();
        let auth = WorkBuddyAuth::load_from_path(&path).unwrap();
        assert_eq!(auth.access_token, "nested");
        assert_eq!(auth.refresh_token.as_deref(), Some("refresh"));
        assert_eq!(auth.uid.as_deref(), Some("u"));
        assert_eq!(auth.request_base_url(), "https://www.workbuddy.cn");
        let missing = dir.path().join("missing.info");
        fs::write(&missing, r#"{"auth":{}}"#).unwrap();
        assert!(matches!(
            WorkBuddyAuth::load_from_path(&missing),
            Err(WorkBuddyAuthError::NotLoggedIn)
        ));
    }

    #[test]
    fn refresh_persistence_preserves_unknown_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workbuddy-desktop.info");
        fs::write(
            &path,
            r#"{"auth":{"accessToken":"old","refreshToken":"old-r"},"keep":{"value":true}}"#,
        )
        .unwrap();
        let mut auth = WorkBuddyAuth::load_from_path(&path).unwrap();
        auth.save_tokens("new".into(), Some("new-r".into()))
            .unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(saved["auth"]["accessToken"], "new");
        assert_eq!(saved["keep"]["value"], true);
    }

    #[test]
    fn arbitrary_domains_fall_back_to_codebuddy() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("workbuddy-desktop.info");
        fs::write(&path, r#"{"accessToken":"token","domain":"evil.example"}"#).unwrap();
        let auth = WorkBuddyAuth::load_from_path(&path).unwrap();
        assert_eq!(auth.request_base_url(), "https://www.codebuddy.cn");
    }
}
