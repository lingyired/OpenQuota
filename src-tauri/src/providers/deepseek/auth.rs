use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

use super::DeepSeekError;

const CONFIG_PATHS: &[&str] = &["~/.config/quota01/deepseek.json"];
const ENVIRONMENT_NAMES: &[&str] = &["DEEPSEEK_USER_TOKEN"];

#[derive(Clone)]
pub struct DeepSeekAuthStore {
    store: ApiKeyStore,
}

impl DeepSeekAuthStore {
    pub fn new() -> Self {
        Self {
            store: ApiKeyStore::new_with_sources("deepseek", ENVIRONMENT_NAMES, CONFIG_PATHS),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(store: ApiKeyStore) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Option<SecretString>, DeepSeekError> {
        self.store.load().map_err(DeepSeekError::CredentialStorage)
    }

    pub fn has_credentials(&self) -> bool {
        self.store.has_credentials()
    }

    pub fn status(&self) -> Result<ApiKeyStatus, DeepSeekError> {
        self.store
            .status()
            .map_err(DeepSeekError::CredentialStorage)
    }

    pub fn save(&self, value: &str) -> Result<(), DeepSeekError> {
        self.store.save(value).map_err(|error| {
            if value.trim().is_empty() {
                DeepSeekError::MissingToken
            } else {
                crate::app_warn!("auth:deepseek", "system credential store write failed");
                DeepSeekError::CredentialStorage(error)
            }
        })
    }

    pub fn delete(&self) -> Result<(), DeepSeekError> {
        self.store.delete().map_err(|error| {
            crate::app_warn!("auth:deepseek", "system credential store delete failed");
            DeepSeekError::CredentialStorage(error)
        })
    }
}

impl Default for DeepSeekAuthStore {
    fn default() -> Self {
        Self::new()
    }
}
