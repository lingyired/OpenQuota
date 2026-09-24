use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use tempfile::NamedTempFile;
use thiserror::Error;

/// 登录文件里 accessToken 的候选路径，按优先级排列。加密检测与明文读取共用同一份
/// 顺序，避免两处不一致导致「明明有明文却被判定为加密」。
const ACCESS_TOKEN_PATHS: &[&[&str]] = &[
    &["auth", "accessToken"],
    &["auth", "access_token"],
    &["accessToken"],
    &["access_token"],
];

/// 其余字段的候选路径同样按优先级排列。集中成常量是为了让 `load_from_path` 只表达
/// 「读哪些字段」，而不是把一串嵌套数组混进取值逻辑里。
const REFRESH_TOKEN_PATHS: &[&[&str]] = &[
    &["auth", "refreshToken"],
    &["auth", "refresh_token"],
    &["refreshToken"],
    &["refresh_token"],
];
const TOKEN_TYPE_PATHS: &[&[&str]] = &[
    &["auth", "tokenType"],
    &["auth", "token_type"],
    &["tokenType"],
    &["token_type"],
];
const DOMAIN_PATHS: &[&[&str]] = &[&["domain"], &["auth", "domain"]];
const UID_PATHS: &[&[&str]] = &[&["uid"], &["account", "uid"], &["account", "id"]];
const ENTERPRISE_ID_PATHS: &[&[&str]] = &[
    &["enterpriseId"],
    &["enterprise_id"],
    &["auth", "enterpriseId"],
    &["auth", "enterprise_id"],
    &["account", "enterpriseId"],
    &["account", "enterprise_id"],
];
const NICKNAME_PATHS: &[&[&str]] = &[
    &["nickname"],
    &["name"],
    &["account", "nickname"],
    &["account", "label"],
];
const EMAIL_PATHS: &[&[&str]] = &[&["email"], &["account", "email"], &["auth", "email"]];
const EXPIRES_AT_PATHS: &[&[&str]] = &[
    &["expiresAt"],
    &["expires_at"],
    &["auth", "expiresAt"],
    &["auth", "expires_at"],
];

#[derive(Debug, Error)]
pub enum WorkBuddyAuthError {
    #[error("WorkBuddy is not logged in.")]
    NotLoggedIn,
    #[error("WorkBuddy login data is invalid.")]
    Invalid,
    #[error("WorkBuddy credentials could not be read or updated.")]
    Storage,
    /// WorkBuddy 5.6 起把 token 写成 `{$wbEncrypted, envelope}` 加密信封，本机又找不到
    /// 可读的明文副本。这不是「未登录」，必须与 `NotLoggedIn` 区分开，否则会把用户
    /// 引向无用的「重新登录」。
    #[error("WorkBuddy 5.6 encrypts its local login data and no readable copy was found.")]
    Encrypted,
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

    /// 从 WorkBuddy 登录文件读取凭据。WorkBuddy 5.6 起把 accessToken 写成
    /// `{$wbEncrypted, envelope}` 加密信封：这是「读不出来」而不是「没登录」，必须报
    /// `Encrypted`，否则凭据探测会把 provider 判成 `Absent` 并自动隐藏，用户根本看不到
    /// 真正的原因。
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

    pub fn has_local_credentials() -> bool {
        Self::has_local_credentials_at(&auth_file_path())
    }

    /// 登录文件存在就说明本机登录过 WorkBuddy/CodeBuddy。即使 WorkBuddy 5.6 之后内容
    /// 读不出来，也必须继续算「有本地凭据」：否则凭据探测会判为 `Absent`、provider 被
    /// 自动禁用隐藏，用户根本看不到「凭据已加密」这个真正的原因。
    pub fn has_local_credentials_at(path: &Path) -> bool {
        path.is_file()
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

/// WorkBuddy 5.6 的加密信封形态：`{"$wbEncrypted": 1, "envelope": "..."}`。
fn is_encrypted_envelope(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_object)
        .is_some_and(|object| object.contains_key("$wbEncrypted"))
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

    /// WorkBuddy 5.6 起把 accessToken 写成 `{$wbEncrypted, envelope}` 加密信封。
    fn encrypted_auth_document(uid: &str) -> String {
        format!(
            r#"{{"account":{{"uid":"{uid}"}},"auth":{{"accessToken":{{"$wbEncrypted":1,"envelope":"ZW5j"}},"tokenType":"Bearer","domain":"www.codebuddy.cn"}}}}"#
        )
    }

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

    #[test]
    fn an_encrypted_login_file_still_counts_as_local_credentials() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("workbuddy-desktop.info");
        fs::write(&auth_path, encrypted_auth_document("uid-1")).unwrap();

        assert!(WorkBuddyAuth::has_local_credentials_at(&auth_path));
        assert!(!WorkBuddyAuth::has_local_credentials_at(
            &dir.path().join("missing.info")
        ));
    }
}
