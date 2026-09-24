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

/// 凭据来源。决定能不能把刷新后的 token 写回磁盘。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkBuddyAuthSource {
    /// WorkBuddy / CodeBuddy 自己写的登录文件：明文凭据，刷新后应写回。
    AuthFile,
    /// workbuddy-switch 的账号库：属于另一个应用，只能只读借用。
    SharedStore,
}

#[derive(Debug, Clone)]
pub struct WorkBuddyAuth {
    pub path: PathBuf,
    pub document: Value,
    pub source: WorkBuddyAuthSource,
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
        Self::load_from_paths(&auth_file_path(), &shared_store_path())
    }

    pub fn load_from_path(path: &Path) -> Result<Self, WorkBuddyAuthError> {
        Self::load_from_paths(path, &shared_store_path())
    }

    /// 从 WorkBuddy 登录文件读取凭据。WorkBuddy 5.6 起把 accessToken 写成
    /// `{$wbEncrypted, envelope}` 加密信封，此时退回只读账号库里的明文副本。
    pub fn load_from_paths(path: &Path, shared_store: &Path) -> Result<Self, WorkBuddyAuthError> {
        let text = fs::read_to_string(path).map_err(|_| WorkBuddyAuthError::NotLoggedIn)?;
        let document: Value =
            serde_json::from_str(&text).map_err(|_| WorkBuddyAuthError::Invalid)?;
        let Some(access_token) =
            first_string(&document, ACCESS_TOKEN_PATHS).filter(|value| !value.is_empty())
        else {
            return if is_encrypted_envelope(first_value(&document, ACCESS_TOKEN_PATHS)) {
                let uid = first_string(
                    &document,
                    &[&["uid"], &["account", "uid"], &["account", "id"]],
                );
                let domain = first_string(&document, &[&["domain"], &["auth", "domain"]])
                    .unwrap_or_default();
                Self::from_shared_store(shared_store, uid.as_deref(), &domain)
                    .ok_or(WorkBuddyAuthError::Encrypted)
            } else {
                Err(WorkBuddyAuthError::NotLoggedIn)
            };
        };
        Ok(Self {
            path: path.to_owned(),
            source: WorkBuddyAuthSource::AuthFile,
            access_token,
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
        })
    }

    /// 只读借用另一个应用的明文副本：必须按 uid 精确配对，取不到就报加密错误，
    /// 绝不能随便挑一个账号（会把别人的用量当成当前账号展示）。
    fn from_shared_store(store: &Path, uid: Option<&str>, domain: &str) -> Option<WorkBuddyAuth> {
        let uid = uid?;
        let parsed: Value = serde_json::from_str(&fs::read_to_string(store).ok()?).ok()?;
        let mut candidates = parsed.as_array()?.iter().filter(|account| {
            account.get("uid").and_then(Value::as_str) == Some(uid)
                && account
                    .get("access_token")
                    .and_then(Value::as_str)
                    .is_some_and(|token| !token.trim().is_empty())
        });
        let first = candidates.next()?;
        let chosen = std::iter::once(first)
            .chain(candidates)
            .find(|account| {
                account
                    .get("domain")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case(domain))
            })
            .unwrap_or(first);
        Some(WorkBuddyAuth {
            path: store.to_owned(),
            document: Value::Null,
            source: WorkBuddyAuthSource::SharedStore,
            access_token: chosen
                .get("access_token")
                .and_then(Value::as_str)?
                .trim()
                .to_owned(),
            refresh_token: chosen
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(str::to_owned),
            token_type: chosen
                .get("token_type")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Bearer")
                .to_owned(),
            domain: chosen
                .get("domain")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            uid: Some(uid.to_owned()),
            enterprise_id: chosen
                .get("enterpriseId")
                .and_then(Value::as_str)
                .map(str::to_owned),
            nickname: chosen
                .get("nickname")
                .and_then(Value::as_str)
                .map(str::to_owned),
            email: chosen
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_owned),
            expires_at: chosen.get("expiresAt").and_then(parse_datetime),
        })
    }

    /// 只读来源（借来的明文副本）不允许写回：那会覆盖别人的凭据，也可能让对方
    /// 的 refresh token 因轮换而失效。刷新失败的提示由 provider 层负责。
    pub fn is_read_only(&self) -> bool {
        self.source != WorkBuddyAuthSource::AuthFile
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
        if self.is_read_only() {
            return Err(WorkBuddyAuthError::Storage);
        }
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

/// workbuddy-switch 的账号库。它自己走 OAuth 登录并保活，库里的 token 是明文，
/// 因此当 WorkBuddy 5.6 的登录文件不可读时，可以按 uid 借用一份只读副本。
///
/// 这是**可选**来源：该应用是第三方工具，可能没装、没登录或没在运行，
/// 所以调用方必须把「借不到」当成正常分支处理，而不是当成缺配置。
pub fn shared_store_path() -> PathBuf {
    home_dir().join(".wb-switch/accounts.json")
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

    fn write_store(dir: &Path, contents: &str) -> PathBuf {
        let path = dir.join("accounts.json");
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn encrypted_auth_file_falls_back_to_the_readable_shared_store() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("workbuddy-desktop.info");
        fs::write(&auth_path, encrypted_auth_document("uid-1")).unwrap();
        let store = write_store(
            dir.path(),
            r#"[{"uid":"uid-1","domain":"www.codebuddy.cn","access_token":"plain-1","refresh_token":"plain-r1","token_type":"Bearer","expiresAt":1795002322225}]"#,
        );

        let auth = WorkBuddyAuth::load_from_paths(&auth_path, &store).unwrap();

        assert_eq!(auth.access_token, "plain-1");
        assert_eq!(auth.refresh_token.as_deref(), Some("plain-r1"));
        assert_eq!(auth.uid.as_deref(), Some("uid-1"));
        assert_eq!(auth.request_base_url(), "https://www.codebuddy.cn");
        assert!(auth.is_read_only());
    }

    #[test]
    fn encrypted_auth_file_without_a_readable_copy_reports_encryption() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("workbuddy-desktop.info");
        fs::write(&auth_path, encrypted_auth_document("uid-1")).unwrap();

        let other_account = write_store(dir.path(), r#"[{"uid":"uid-2","access_token":"other"}]"#);
        assert!(matches!(
            WorkBuddyAuth::load_from_paths(&auth_path, &other_account),
            Err(WorkBuddyAuthError::Encrypted)
        ));

        let missing = dir.path().join("missing.json");
        assert!(matches!(
            WorkBuddyAuth::load_from_paths(&auth_path, &missing),
            Err(WorkBuddyAuthError::Encrypted)
        ));
    }

    #[test]
    fn plaintext_auth_file_wins_over_the_shared_store() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("workbuddy-desktop.info");
        fs::write(
            &auth_path,
            r#"{"account":{"uid":"uid-1"},"auth":{"accessToken":"from-auth"}}"#,
        )
        .unwrap();
        let store = write_store(
            dir.path(),
            r#"[{"uid":"uid-1","access_token":"from-store"}]"#,
        );

        let auth = WorkBuddyAuth::load_from_paths(&auth_path, &store).unwrap();

        assert_eq!(auth.access_token, "from-auth");
        assert!(!auth.is_read_only());
    }

    #[test]
    fn shared_store_credentials_are_never_written_back() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("workbuddy-desktop.info");
        let original = encrypted_auth_document("uid-1");
        fs::write(&auth_path, &original).unwrap();
        let store = write_store(
            dir.path(),
            r#"[{"uid":"uid-1","access_token":"plain-1","refresh_token":"plain-r1"}]"#,
        );

        let mut auth = WorkBuddyAuth::load_from_paths(&auth_path, &store).unwrap();
        let saved = auth.save_tokens("rotated".into(), Some("rotated-r".into()));

        assert!(matches!(saved, Err(WorkBuddyAuthError::Storage)));
        assert_eq!(fs::read_to_string(&auth_path).unwrap(), original);
        assert_eq!(auth.access_token, "plain-1");
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
