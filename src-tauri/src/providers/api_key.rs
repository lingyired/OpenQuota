use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Once},
};

use crate::models::ApiKeyStatus;
use zeroize::Zeroizing;

use super::credential_vault;

pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    pub fn new(value: Vec<u8>) -> Self {
        Self(Zeroizing::new(value))
    }

    fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

pub struct SecretString(Zeroizing<String>);

impl SecretString {
    fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

pub trait SecretBackend: Send + Sync {
    fn read(&self, account: &str) -> Result<Option<SecretBytes>, String>;
    fn exists(&self, account: &str) -> Result<bool, String> {
        self.read(account).map(|value| value.is_some())
    }
    fn write(&self, account: &str, value: &[u8]) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

#[derive(Default)]
struct VaultSecretBackend;

impl SecretBackend for VaultSecretBackend {
    fn read(&self, account: &str) -> Result<Option<SecretBytes>, String> {
        credential_vault::read(account).map(|value| value.map(SecretBytes::new))
    }

    fn exists(&self, account: &str) -> Result<bool, String> {
        credential_vault::contains(account)
    }

    fn write(&self, account: &str, value: &[u8]) -> Result<(), String> {
        credential_vault::write(account, value)
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        credential_vault::delete(account)
    }
}

/// 完全绕开系统凭据库的后端：读一律「没有」，写/删一律明确报错。
///
/// 用于测试与无头场景（见 [`keychain_disabled`]）：这些场景下凭据只应来自环境变量或
/// 配置文件，而本地未签名的构建每次重建都会让钥匙串访问许可失效，于是每个条目都会
/// 弹一次系统授权框。明确报错好过偷偷弹窗——用户能看懂发生了什么。
struct DisabledSecrets;

impl SecretBackend for DisabledSecrets {
    fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
        Ok(None)
    }

    fn exists(&self, _account: &str) -> Result<bool, String> {
        Ok(false)
    }

    fn write(&self, _account: &str, _value: &[u8]) -> Result<(), String> {
        Err(KEYCHAIN_DISABLED_MESSAGE.to_owned())
    }

    fn delete(&self, _account: &str) -> Result<(), String> {
        Err(KEYCHAIN_DISABLED_MESSAGE.to_owned())
    }
}

const KEYCHAIN_ENVIRONMENT: &str = "QUOTA01_NO_KEYCHAIN";
const KEYCHAIN_SENTINEL: &str = "~/.config/quota01/no-keychain";
const KEYCHAIN_DISABLED_MESSAGE: &str = "The system credential store is switched off (no-keychain); supply the credential through an environment variable or the provider's config file instead.";

/// 系统凭据库是否被整体停用：设了 `QUOTA01_NO_KEYCHAIN`，或存在 `~/.config/quota01/no-keychain`。
///
/// 存在这两个开关之一时，所有 provider 都只从环境变量/配置文件取凭据，绝不读取或写入
/// 系统钥匙串，因此本地重建不会再触发授权弹窗。
pub fn keychain_disabled() -> bool {
    keychain_disabled_by(
        std::env::var_os(KEYCHAIN_ENVIRONMENT).as_deref(),
        &expand_home(KEYCHAIN_SENTINEL),
    )
}

/// 判定与真实 IO 分开，好把「什么算停用」钉在测试里。
fn keychain_disabled_by(flag: Option<&OsStr>, sentinel: &Path) -> bool {
    if flag.is_some_and(|value| !value.is_empty() && value != "0") {
        return true;
    }
    sentinel.is_file()
}

fn process_secrets() -> Arc<dyn SecretBackend> {
    if !keychain_disabled() {
        return Arc::new(VaultSecretBackend);
    }
    static ANNOUNCED: Once = Once::new();
    ANNOUNCED.call_once(|| {
        crate::app_info!(
            "auth",
            "system credential store is off ({KEYCHAIN_ENVIRONMENT} or {KEYCHAIN_SENTINEL}); credentials come from environment variables and config files only"
        );
    });
    Arc::new(DisabledSecrets)
}

pub trait EnvironmentReader: Send + Sync {
    fn value(&self, name: &str) -> Option<String>;
}

#[derive(Default)]
struct ProcessEnvironment;

impl EnvironmentReader for ProcessEnvironment {
    fn value(&self, name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }
}

pub trait ConfigFileReader: Send + Sync {
    fn read(&self, path: &str) -> Option<SecretBytes>;
}

#[derive(Default)]
struct ProcessConfigFiles;

impl ConfigFileReader for ProcessConfigFiles {
    fn read(&self, path: &str) -> Option<SecretBytes> {
        fs::read(expand_home(path)).ok().map(SecretBytes::new)
    }
}

#[derive(Clone)]
pub struct ApiKeyStore {
    provider_id: String,
    /// Vault account names a provider used before it was renamed. Read-only
    /// fallbacks: they keep a saved key working across a rename, while writes
    /// and deletes always target the current `provider_id`.
    legacy_provider_ids: Vec<String>,
    environment_names: Vec<String>,
    config_paths: Vec<String>,
    /// 配置文件是否按「整份文档」读取。默认是「一个 key」的语义：从 JSON 里挑
    /// `apiKey` 一类的字段。WorkBuddy 的凭据是一份会话文档，形状对不上，所以要能
    /// 把整份文件当凭据，而不是被 `key_from_config` 挑字段挑没了。
    config_documents: bool,
    secrets: Arc<dyn SecretBackend>,
    environment: Arc<dyn EnvironmentReader>,
    config_files: Arc<dyn ConfigFileReader>,
}

impl ApiKeyStore {
    pub fn new_with_sources(
        provider_id: &str,
        environment_names: &[&str],
        config_paths: &[&str],
    ) -> Self {
        Self::new_with_sources_and_legacy(provider_id, &[], environment_names, config_paths)
    }

    pub fn new_with_sources_and_legacy(
        provider_id: &str,
        legacy_provider_ids: &[&str],
        environment_names: &[&str],
        config_paths: &[&str],
    ) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            legacy_provider_ids: legacy_provider_ids
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            environment_names: environment_names
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            config_paths: config_paths
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            config_documents: false,
            secrets: process_secrets(),
            environment: Arc::new(ProcessEnvironment),
            config_files: Arc::new(ProcessConfigFiles),
        }
    }

    /// 配置文件按整份文档读取，而不是从里面挑一个字段。
    ///
    /// 用于凭据本身就是一份文档的 provider：WorkBuddy 的会话是 JSON 文档，
    /// 被 `key_from_config` 当成 key 信封去挑字段只会挑成空。
    pub fn with_config_documents(mut self) -> Self {
        self.config_documents = true;
        self
    }

    #[cfg(test)]
    pub fn with_backends(
        provider_id: &str,
        environment_name: &str,
        secrets: Arc<dyn SecretBackend>,
        environment: Arc<dyn EnvironmentReader>,
    ) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            legacy_provider_ids: Vec::new(),
            environment_names: vec![environment_name.to_owned()],
            config_paths: Vec::new(),
            config_documents: false,
            secrets,
            environment,
            config_files: Arc::new(ProcessConfigFiles),
        }
    }

    #[cfg(test)]
    pub fn with_legacy_provider_ids(mut self, legacy_provider_ids: &[&str]) -> Self {
        self.legacy_provider_ids = legacy_provider_ids
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        self
    }

    #[cfg(test)]
    pub fn with_source_backends(
        provider_id: &str,
        environment_names: &[&str],
        config_paths: &[&str],
        secrets: Arc<dyn SecretBackend>,
        environment: Arc<dyn EnvironmentReader>,
        config_files: Arc<dyn ConfigFileReader>,
    ) -> Self {
        Self {
            provider_id: provider_id.to_owned(),
            legacy_provider_ids: Vec::new(),
            environment_names: environment_names
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            config_paths: config_paths
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            config_documents: false,
            secrets,
            environment,
            config_files,
        }
    }

    pub fn load(&self) -> Result<Option<SecretString>, String> {
        match self.saved_key() {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => Ok(self.external_key().map(|(value, _)| value)),
            Err(error) => match self.external_key() {
                Some((value, _)) => {
                    report_external_fallback(&self.provider_id, &error);
                    Ok(Some(value))
                }
                None => Err(error),
            },
        }
    }

    pub fn has_credentials(&self) -> bool {
        self.external_key().is_some() || self.saved_key_exists().unwrap_or(false)
    }

    pub fn status(&self) -> Result<ApiKeyStatus, String> {
        let external = self.external_key().map(|(_, status)| status);
        let saved = match self.saved_key_exists() {
            Ok(value) => value,
            Err(error) if external.is_some() => {
                report_external_fallback(&self.provider_id, &error);
                return Ok(external.expect("external source checked above"));
            }
            Err(error) => return Err(error),
        };
        Ok(if saved {
            if external.is_some() {
                ApiKeyStatus::OverrideActive
            } else {
                ApiKeyStatus::Saved
            }
        } else {
            external.unwrap_or(ApiKeyStatus::NotSet)
        })
    }

    pub fn save(&self, value: &str) -> Result<(), String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("Enter an API key before saving.".into());
        }
        self.secrets.write(&self.provider_id, value.as_bytes())
    }

    pub fn delete(&self) -> Result<(), String> {
        self.secrets.delete(&self.provider_id)?;
        for account in &self.legacy_provider_ids {
            if self.secrets.exists(account)? {
                self.secrets.delete(account)?;
            }
        }
        Ok(())
    }

    fn saved_accounts(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.provider_id.as_str())
            .chain(self.legacy_provider_ids.iter().map(String::as_str))
    }

    fn saved_key(&self) -> Result<Option<SecretString>, String> {
        for account in self.saved_accounts() {
            let Some(value) = self.secrets.read(account)? else {
                continue;
            };
            let value = std::str::from_utf8(value.as_slice())
                .map_err(|_| "The saved API key has an unsupported encoding.".to_owned())?;
            if let Some(value) = non_empty(value.to_owned()) {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    fn saved_key_exists(&self) -> Result<bool, String> {
        for account in self.saved_accounts() {
            if self.secrets.exists(account)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn environment_key(&self) -> Option<SecretString> {
        self.environment_names
            .iter()
            .find_map(|name| self.environment.value(name).and_then(non_empty))
    }

    fn config_key(&self) -> Option<SecretString> {
        self.config_paths.iter().find_map(|path| {
            self.config_files.read(path).and_then(|value| {
                if self.config_documents {
                    document_from_config(value.as_slice())
                } else {
                    key_from_config(value.as_slice())
                }
            })
        })
    }

    fn external_key(&self) -> Option<(SecretString, ApiKeyStatus)> {
        self.config_key()
            .map(|value| (value, ApiKeyStatus::FromConfig))
            .or_else(|| {
                self.environment_key()
                    .map(|value| (value, ApiKeyStatus::FromEnvironment))
            })
    }
}

fn report_external_fallback(provider_id: &str, error: &str) {
    crate::app_warn!(
        &format!("auth:{provider_id}"),
        "system credential store unavailable; using external API key source ({error})"
    );
}

fn key_from_config(bytes: &[u8]) -> Option<SecretString> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if text.starts_with('{') {
        let object = serde_json::from_str::<serde_json::Value>(text)
            .ok()?
            .as_object()?
            .clone();
        return [
            "apiKey",
            "api_key",
            "userToken",
            "user_token",
            "token",
            "key",
        ]
        .iter()
        .find_map(|name| object.get(*name)?.as_str().map(str::to_owned))
        .and_then(non_empty);
    }
    non_empty(text.to_owned())
}

/// 文档式配置来源：整份文件就是凭据，原样取出（保留它的 JSON 结构）。
fn document_from_config(bytes: &[u8]) -> Option<SecretString> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    non_empty(text.to_owned())
}

fn expand_home(path: &str) -> PathBuf {
    let Some(relative) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) else {
        return PathBuf::from(path);
    };
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(relative)
}

fn non_empty(value: String) -> Option<SecretString> {
    let value = SecretString::new(value);
    let trimmed = value.as_str().trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() == value.len() {
        Some(value)
    } else {
        Some(SecretString::new(trimmed.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use crate::models::ApiKeyStatus;

    use super::{
        key_from_config, keychain_disabled_by, ApiKeyStore, ConfigFileReader, DisabledSecrets,
        EnvironmentReader, SecretBackend, SecretBytes,
    };
    use std::ffi::OsStr;

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

    struct MemoryEnvironment(HashMap<String, String>);

    impl EnvironmentReader for MemoryEnvironment {
        fn value(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    struct MemoryConfigFiles(HashMap<String, Vec<u8>>);

    impl ConfigFileReader for MemoryConfigFiles {
        fn read(&self, path: &str) -> Option<SecretBytes> {
            self.0.get(path).cloned().map(SecretBytes::new)
        }
    }

    struct ReadErrorSecrets;

    impl SecretBackend for ReadErrorSecrets {
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

    struct ExistsOnlySecrets;

    impl SecretBackend for ExistsOnlySecrets {
        fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
            Err("The secret must not be read for a presence check.".into())
        }

        fn exists(&self, _account: &str) -> Result<bool, String> {
            Ok(true)
        }

        fn write(&self, _account: &str, _value: &[u8]) -> Result<(), String> {
            unreachable!()
        }

        fn delete(&self, _account: &str) -> Result<(), String> {
            unreachable!()
        }
    }

    fn store(secrets: Arc<MemorySecrets>, environment: &[(&str, &str)]) -> ApiKeyStore {
        ApiKeyStore::with_backends(
            "provider",
            "PROVIDER_API_KEY",
            secrets,
            Arc::new(MemoryEnvironment(
                environment
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                    .collect(),
            )),
        )
    }

    #[test]
    fn saved_key_overrides_environment_and_delete_falls_back() {
        let secrets = Arc::new(MemorySecrets::default());
        let store = store(secrets, &[("PROVIDER_API_KEY", " environment-key ")]);
        assert_eq!(store.status().unwrap(), ApiKeyStatus::FromEnvironment);
        assert_eq!(
            store.load().unwrap().as_ref().map(|value| value.as_str()),
            Some("environment-key")
        );

        store.save(" saved-key ").unwrap();
        assert_eq!(store.status().unwrap(), ApiKeyStatus::OverrideActive);
        assert_eq!(
            store.load().unwrap().as_ref().map(|value| value.as_str()),
            Some("saved-key")
        );

        store.delete().unwrap();
        assert_eq!(store.status().unwrap(), ApiKeyStatus::FromEnvironment);
        assert_eq!(
            store.load().unwrap().as_ref().map(|value| value.as_str()),
            Some("environment-key")
        );
    }

    #[test]
    fn saved_only_and_empty_states_are_reported_without_exposing_the_key() {
        let secrets = Arc::new(MemorySecrets::default());
        let store = store(secrets, &[]);
        assert_eq!(store.status().unwrap(), ApiKeyStatus::NotSet);
        assert!(store.save("  ").is_err());

        store.save("secret").unwrap();
        assert_eq!(store.status().unwrap(), ApiKeyStatus::Saved);
    }

    #[test]
    fn environment_key_remains_available_when_the_system_store_cannot_be_read() {
        let store = ApiKeyStore::with_backends(
            "provider",
            "PROVIDER_API_KEY",
            Arc::new(ReadErrorSecrets),
            Arc::new(MemoryEnvironment(HashMap::from([(
                "PROVIDER_API_KEY".into(),
                " environment-key ".into(),
            )]))),
        );

        assert_eq!(store.status().unwrap(), ApiKeyStatus::FromEnvironment);
        assert_eq!(
            store.load().unwrap().as_ref().map(|value| value.as_str()),
            Some("environment-key")
        );
    }

    #[test]
    fn credential_store_errors_are_not_hidden_without_an_environment_key() {
        let store = ApiKeyStore::with_backends(
            "provider",
            "PROVIDER_API_KEY",
            Arc::new(ReadErrorSecrets),
            Arc::new(MemoryEnvironment(HashMap::new())),
        );

        assert_eq!(
            store.status().unwrap_err(),
            "System credential store unavailable."
        );
        assert_eq!(
            store.load().err().as_deref(),
            Some("System credential store unavailable.")
        );
    }

    #[test]
    fn api_key_status_uses_a_presence_probe_without_reading_the_secret() {
        let store = ApiKeyStore::with_backends(
            "provider",
            "PROVIDER_API_KEY",
            Arc::new(ExistsOnlySecrets),
            Arc::new(MemoryEnvironment(HashMap::new())),
        );

        assert!(store.has_credentials());
        assert_eq!(store.status().unwrap(), ApiKeyStatus::Saved);
    }

    #[test]
    fn the_keychain_switch_follows_the_environment_and_the_sentinel_file() {
        let directory = tempfile::tempdir().unwrap();
        let sentinel = directory.path().join("no-keychain");

        assert!(!keychain_disabled_by(None, &sentinel));
        assert!(!keychain_disabled_by(Some(OsStr::new("0")), &sentinel));
        assert!(keychain_disabled_by(Some(OsStr::new("1")), &sentinel));

        std::fs::write(&sentinel, b"").unwrap();
        assert!(keychain_disabled_by(None, &sentinel));
    }

    #[test]
    fn a_disabled_keychain_supplies_nothing_and_refuses_writes() {
        let store = ApiKeyStore::with_source_backends(
            "provider",
            &[],
            &[],
            Arc::new(DisabledSecrets),
            Arc::new(MemoryEnvironment(HashMap::new())),
            Arc::new(MemoryConfigFiles(HashMap::new())),
        );

        assert!(!store.has_credentials());
        assert!(store.load().unwrap().is_none());
        assert!(
            store.save("secret").is_err(),
            "a switched-off keychain must say so instead of silently dropping the secret"
        );
    }

    #[test]
    fn a_document_config_source_keeps_the_whole_file() {
        // 文档式来源（WorkBuddy 的会话）不能被「挑字段」的语义吃掉：
        // 同一份内容按 key 语义会取不到任何字段。
        let document = r#"{"access_token":"access-1","domain":"www.codebuddy.cn"}"#;
        let config_files = Arc::new(MemoryConfigFiles(HashMap::from([(
            "~/workbuddy.json".into(),
            document.as_bytes().to_vec(),
        )])));

        let as_key = ApiKeyStore::with_source_backends(
            "workbuddy-cn-session",
            &[],
            &["~/workbuddy.json"],
            Arc::new(MemorySecrets::default()),
            Arc::new(MemoryEnvironment(HashMap::new())),
            config_files.clone(),
        );
        assert!(as_key.load().unwrap().is_none());

        let as_document = ApiKeyStore::with_source_backends(
            "workbuddy-cn-session",
            &[],
            &["~/workbuddy.json"],
            Arc::new(MemorySecrets::default()),
            Arc::new(MemoryEnvironment(HashMap::new())),
            config_files,
        )
        .with_config_documents();
        assert_eq!(
            as_document
                .load()
                .unwrap()
                .as_ref()
                .map(|value| value.as_str()),
            Some(document)
        );
    }

    #[test]
    fn config_file_precedes_environment_and_saved_key_can_override_it() {
        let secrets = Arc::new(MemorySecrets::default());
        let store = ApiKeyStore::with_source_backends(
            "provider",
            &["PRIMARY_KEY", "ALTERNATE_KEY"],
            &["~/first.json", "~/second.json"],
            secrets,
            Arc::new(MemoryEnvironment(HashMap::from([
                ("PRIMARY_KEY".into(), "environment-key".into()),
                ("ALTERNATE_KEY".into(), "alternate-key".into()),
            ]))),
            Arc::new(MemoryConfigFiles(HashMap::from([(
                "~/second.json".into(),
                br#"{"api_key":"config-key"}"#.to_vec(),
            )]))),
        );

        assert_eq!(store.status().unwrap(), ApiKeyStatus::FromConfig);
        assert_eq!(
            store.load().unwrap().as_ref().map(|value| value.as_str()),
            Some("config-key")
        );

        store.save("saved-key").unwrap();
        assert_eq!(store.status().unwrap(), ApiKeyStatus::OverrideActive);
        assert_eq!(
            store.load().unwrap().as_ref().map(|value| value.as_str()),
            Some("saved-key")
        );
    }

    #[test]
    fn alternate_environment_name_and_plain_text_config_are_supported() {
        let secrets = Arc::new(MemorySecrets::default());
        let environment_store = ApiKeyStore::with_source_backends(
            "provider",
            &["PRIMARY_KEY", "ALTERNATE_KEY"],
            &[],
            secrets.clone(),
            Arc::new(MemoryEnvironment(HashMap::from([(
                "ALTERNATE_KEY".into(),
                " alternate-key ".into(),
            )]))),
            Arc::new(MemoryConfigFiles(HashMap::new())),
        );
        assert_eq!(
            environment_store
                .load()
                .unwrap()
                .as_ref()
                .map(|value| value.as_str()),
            Some("alternate-key")
        );

        let config_store = ApiKeyStore::with_source_backends(
            "provider",
            &[],
            &["~/key.txt"],
            secrets,
            Arc::new(MemoryEnvironment(HashMap::new())),
            Arc::new(MemoryConfigFiles(HashMap::from([(
                "~/key.txt".into(),
                b" plain-text-key \n".to_vec(),
            )]))),
        );
        assert_eq!(config_store.status().unwrap(), ApiKeyStatus::FromConfig);
        assert_eq!(
            config_store
                .load()
                .unwrap()
                .as_ref()
                .map(|value| value.as_str()),
            Some("plain-text-key")
        );
    }

    #[test]
    fn config_file_accepts_token_style_aliases() {
        for name in [
            "apiKey",
            "api_key",
            "userToken",
            "user_token",
            "token",
            "key",
        ] {
            let body = format!(r#"{{"{name}":" value-{name} "}}"#);
            assert_eq!(
                key_from_config(body.as_bytes()).unwrap().as_str(),
                format!("value-{name}")
            );
        }
    }

    #[test]
    fn legacy_vault_accounts_keep_a_saved_key_alive_across_a_rename() {
        let secrets = Arc::new(MemorySecrets::default());
        secrets
            .write("kimi", b"legacy-key")
            .expect("seed the legacy account");
        let store = || {
            ApiKeyStore::with_source_backends(
                "kimi-cn",
                &["KIMI_API_KEY"],
                &[],
                secrets.clone(),
                Arc::new(MemoryEnvironment(HashMap::new())),
                Arc::new(MemoryConfigFiles(HashMap::new())),
            )
            .with_legacy_provider_ids(&["kimi"])
        };

        let legacy = store();
        assert_eq!(legacy.load().unwrap().unwrap().as_str(), "legacy-key");
        assert_eq!(legacy.status().unwrap(), ApiKeyStatus::Saved);
        assert!(legacy.has_credentials());

        legacy.save("replacement").unwrap();
        assert_eq!(store().load().unwrap().unwrap().as_str(), "replacement");

        store().delete().unwrap();
        assert!(store().load().unwrap().is_none());
        assert_eq!(store().status().unwrap(), ApiKeyStatus::NotSet);
        assert!(!store().has_credentials());
    }
}
