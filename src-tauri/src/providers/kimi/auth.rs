use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

use super::KimiError;

const CONFIG_PATHS: &[&str] = &["~/.config/quota01/kimi.json"];
const ENVIRONMENT_NAMES: &[&str] = &["KIMI_API_KEY"];
/// The provider id used before the mainland-China marker was added.
const LEGACY_PROVIDER_IDS: &[&str] = &["kimi"];

#[derive(Clone)]
pub struct KimiAuthStore {
    store: ApiKeyStore,
}

impl KimiAuthStore {
    pub fn new() -> Self {
        Self {
            store: ApiKeyStore::new_with_sources_and_legacy(
                "kimi-cn",
                LEGACY_PROVIDER_IDS,
                ENVIRONMENT_NAMES,
                CONFIG_PATHS,
            ),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(store: ApiKeyStore) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Option<SecretString>, KimiError> {
        self.store.load().map_err(KimiError::CredentialStorage)
    }

    pub fn has_local_credentials(&self) -> bool {
        self.store.has_credentials()
    }

    pub fn status(&self) -> Result<ApiKeyStatus, KimiError> {
        self.store.status().map_err(KimiError::CredentialStorage)
    }

    pub fn save(&self, value: &str) -> Result<(), KimiError> {
        self.store.save(value).map_err(|error| {
            if value.trim().is_empty() {
                KimiError::MissingKey
            } else {
                crate::app_warn!("auth:kimi", "system credential store write failed");
                KimiError::CredentialStorage(error)
            }
        })
    }

    pub fn delete(&self) -> Result<(), KimiError> {
        self.store.delete().map_err(|error| {
            crate::app_warn!("auth:kimi", "system credential store delete failed");
            KimiError::CredentialStorage(error)
        })
    }
}

impl Default for KimiAuthStore {
    fn default() -> Self {
        Self::new()
    }
}
