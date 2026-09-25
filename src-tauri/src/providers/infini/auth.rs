use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

use super::InfiniError;

const CONFIG_PATHS: &[&str] = &["~/.config/quota01/infini.json"];
const ENVIRONMENT_NAMES: &[&str] = &["INFINI_API_KEY"];

#[derive(Clone)]
pub struct InfiniAuthStore {
    store: ApiKeyStore,
}

impl InfiniAuthStore {
    pub fn new() -> Self {
        Self {
            store: ApiKeyStore::new_with_sources("infini", ENVIRONMENT_NAMES, CONFIG_PATHS),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(store: ApiKeyStore) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Option<SecretString>, InfiniError> {
        self.store
            .load()
            .map_err(|_| InfiniError::CredentialStorage)
    }

    pub fn has_local_credentials(&self) -> bool {
        self.store.has_credentials()
    }

    pub fn status(&self) -> Result<ApiKeyStatus, InfiniError> {
        self.store
            .status()
            .map_err(|_| InfiniError::CredentialStorage)
    }

    pub fn save(&self, value: &str) -> Result<(), InfiniError> {
        self.store.save(value).map_err(|_| {
            if value.trim().is_empty() {
                InfiniError::MissingKey
            } else {
                crate::app_warn!("auth:infini", "system credential store write failed");
                InfiniError::CredentialStorage
            }
        })
    }

    pub fn delete(&self) -> Result<(), InfiniError> {
        self.store.delete().map_err(|_| {
            crate::app_warn!("auth:infini", "system credential store delete failed");
            InfiniError::CredentialStorage
        })
    }
}

impl Default for InfiniAuthStore {
    fn default() -> Self {
        Self::new()
    }
}
