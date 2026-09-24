use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

use super::{SiliconFlowError, Site};

#[derive(Clone)]
pub struct SiliconFlowAuthStore {
    site: Site,
    store: ApiKeyStore,
}

impl SiliconFlowAuthStore {
    pub fn new(site: Site) -> Self {
        Self::with_store(
            site,
            ApiKeyStore::new_with_sources(site.id(), site.environment_names(), site.config_paths()),
        )
    }

    pub(super) fn with_store(site: Site, store: ApiKeyStore) -> Self {
        Self { site, store }
    }

    pub fn load(&self) -> Result<Option<SecretString>, SiliconFlowError> {
        self.store
            .load()
            .map_err(|_| SiliconFlowError::CredentialStorage(self.site))
    }

    pub fn has_local_credentials(&self) -> bool {
        self.store.has_credentials()
    }

    pub fn status(&self) -> Result<ApiKeyStatus, SiliconFlowError> {
        self.store
            .status()
            .map_err(|_| SiliconFlowError::CredentialStorage(self.site))
    }

    pub fn save(&self, value: &str) -> Result<(), SiliconFlowError> {
        self.store.save(value).map_err(|_| {
            if value.trim().is_empty() {
                SiliconFlowError::MissingKey(self.site)
            } else {
                crate::app_warn!("auth:siliconflow", "system credential store write failed");
                SiliconFlowError::CredentialStorage(self.site)
            }
        })
    }

    pub fn delete(&self) -> Result<(), SiliconFlowError> {
        self.store.delete().map_err(|_| {
            crate::app_warn!("auth:siliconflow", "system credential store delete failed");
            SiliconFlowError::CredentialStorage(self.site)
        })
    }
}

impl Default for SiliconFlowAuthStore {
    fn default() -> Self {
        Self::new(Site::Global)
    }
}
