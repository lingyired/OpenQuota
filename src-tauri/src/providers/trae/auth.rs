use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

use super::TraeError;

const ENVIRONMENT_NAMES: &[&str] = &["TRAE_CN_SESSION"];

#[derive(Clone)]
pub struct TraeAuthStore {
    store: ApiKeyStore,
}

impl TraeAuthStore {
    pub fn new() -> Self {
        Self {
            store: ApiKeyStore::new_with_sources("trae-cn", ENVIRONMENT_NAMES, &[]),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(store: ApiKeyStore) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Option<SecretString>, TraeError> {
        self.store.load().map_err(|_| TraeError::CredentialStorage)
    }

    pub fn has_credentials(&self) -> bool {
        self.store.has_credentials()
    }

    pub fn status(&self) -> Result<ApiKeyStatus, TraeError> {
        self.store
            .status()
            .map_err(|_| TraeError::CredentialStorage)
    }

    pub fn save(&self, value: &str) -> Result<(), TraeError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(TraeError::SessionMissing);
        }
        self.store
            .save(value)
            .map_err(|_| TraeError::CredentialStorage)
    }

    pub fn delete(&self) -> Result<(), TraeError> {
        self.store
            .delete()
            .map_err(|_| TraeError::CredentialStorage)
    }
}

impl Default for TraeAuthStore {
    fn default() -> Self {
        Self::new()
    }
}
