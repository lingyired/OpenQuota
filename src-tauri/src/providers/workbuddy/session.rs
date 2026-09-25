use serde::{Deserialize, Serialize};

use crate::{models::ApiKeyStatus, providers::api_key::ApiKeyStore};

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
        match self
            .store
            .load()
            .map_err(|_| WorkBuddySessionError::Storage)?
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::api_key::{ApiKeyStore, EnvironmentReader, SecretBackend, SecretBytes};

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

    /// 一个永远失败的 vault 后端：`save` 的失败路径正是登录成功后落库时决定
    /// 「已连接」还是「失败」的地方，必须在 store 这一层被钉住。
    struct FailingSecrets;

    impl SecretBackend for FailingSecrets {
        fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
            Err("The system credential store is unavailable.".to_owned())
        }

        fn write(&self, _account: &str, _value: &[u8]) -> Result<(), String> {
            Err("The system credential store is unavailable.".to_owned())
        }

        fn delete(&self, _account: &str) -> Result<(), String> {
            Err("The system credential store is unavailable.".to_owned())
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

    /// 校验发生在写入之前：被 `from_json` 拒绝的文档必须原样退回，
    /// 不能以任何形式落到 vault 里（否则一次被拒绝的保存会污染已存会话的状态）。
    #[test]
    fn a_rejected_save_leaves_the_vault_untouched() {
        for document in [r#"{"access_token":"  "}"#, r#"{"refresh_token":"r"}"#] {
            let subject = store(None);

            assert!(matches!(
                subject.save(document),
                Err(WorkBuddySessionError::Malformed)
            ));
            assert_eq!(subject.status().unwrap(), ApiKeyStatus::NotSet);
            assert_eq!(subject.load().unwrap(), None);
        }
    }

    /// vault 的读 / 写 / 删失败一律映射成 `Storage`，而不是 `Malformed`：
    /// 两者在界面上是「凭据存储不可用」与「登录数据无效」两种不同的出路。
    #[test]
    fn a_vault_failure_maps_to_storage_at_the_store_level() {
        let subject = WorkBuddySessionStore::with_store(ApiKeyStore::with_backends(
            VAULT_ACCOUNT,
            "WORKBUDDY_SESSION",
            std::sync::Arc::new(FailingSecrets),
            std::sync::Arc::new(EmptyEnvironment),
        ));
        let document = session().to_json().unwrap();

        assert!(matches!(
            subject.save(&document),
            Err(WorkBuddySessionError::Storage)
        ));
        assert!(matches!(
            subject.load(),
            Err(WorkBuddySessionError::Storage)
        ));
        assert!(matches!(
            subject.status(),
            Err(WorkBuddySessionError::Storage)
        ));
        assert!(matches!(
            subject.delete(),
            Err(WorkBuddySessionError::Storage)
        ));
    }

    #[test]
    fn store_round_trip_uses_the_vault() {
        let subject = store(None);
        assert_eq!(subject.load().unwrap(), None);

        subject.save(&session().to_json().unwrap()).unwrap();
        assert_eq!(subject.load().unwrap(), Some(session()));
    }

    #[test]
    fn a_stored_document_that_is_not_a_session_reports_malformed() {
        let subject = store(Some("not json"));
        assert!(matches!(
            subject.load(),
            Err(WorkBuddySessionError::Malformed)
        ));
    }

    #[test]
    fn status_and_delete_round_trip_through_the_store() {
        let subject = store(None);
        assert_eq!(subject.status().unwrap(), ApiKeyStatus::NotSet);

        subject.save(&session().to_json().unwrap()).unwrap();
        assert_eq!(subject.status().unwrap(), ApiKeyStatus::Saved);

        subject.delete().unwrap();
        assert_eq!(subject.status().unwrap(), ApiKeyStatus::NotSet);
        assert_eq!(subject.load().unwrap(), None);
    }
}
