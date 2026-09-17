use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

use super::{MiniMaxError, Site};

#[derive(Clone)]
pub struct MiniMaxAuthStore {
    site: Site,
    store: ApiKeyStore,
}

impl MiniMaxAuthStore {
    pub fn new(site: Site) -> Self {
        Self::with_store(
            site,
            ApiKeyStore::new_with_sources(site.id(), site.environment_names(), site.config_paths()),
        )
    }

    pub(super) fn with_store(site: Site, store: ApiKeyStore) -> Self {
        Self { site, store }
    }

    pub fn load(&self) -> Result<Option<SecretString>, MiniMaxError> {
        self.store
            .load()
            .map_err(|_| MiniMaxError::CredentialStorage(self.site))
    }

    pub fn has_local_credentials(&self) -> bool {
        self.store.has_credentials()
    }

    pub fn status(&self) -> Result<ApiKeyStatus, MiniMaxError> {
        self.store
            .status()
            .map_err(|_| MiniMaxError::CredentialStorage(self.site))
    }

    pub fn save(&self, value: &str) -> Result<(), MiniMaxError> {
        self.store.save(value).map_err(|_| {
            if value.trim().is_empty() {
                MiniMaxError::MissingKey(self.site)
            } else {
                crate::app_warn!("auth:minimax", "system credential store write failed");
                MiniMaxError::CredentialStorage(self.site)
            }
        })
    }

    pub fn delete(&self) -> Result<(), MiniMaxError> {
        self.store.delete().map_err(|_| {
            crate::app_warn!("auth:minimax", "system credential store delete failed");
            MiniMaxError::CredentialStorage(self.site)
        })
    }
}

impl Default for MiniMaxAuthStore {
    fn default() -> Self {
        Self::new(Site::Global)
    }
}
