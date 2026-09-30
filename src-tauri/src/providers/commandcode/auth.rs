use crate::{
    models::ApiKeyStatus,
    providers::api_key::{ApiKeyStore, SecretString},
};

use super::CommandCodeError;

const CONFIG_PATHS: &[&str] = &["~/.config/quota01/commandcode.json"];
const ENVIRONMENT_NAMES: &[&str] = &["COMMANDCODE_API_KEY"];

#[derive(Clone)]
pub struct CommandCodeAuthStore {
    store: ApiKeyStore,
}

impl CommandCodeAuthStore {
    pub fn new() -> Self {
        Self {
            store: ApiKeyStore::new_with_sources("commandcode", ENVIRONMENT_NAMES, CONFIG_PATHS),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(store: ApiKeyStore) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Option<SecretString>, CommandCodeError> {
        self.store
            .load()
            .map_err(CommandCodeError::CredentialStorage)
    }

    pub fn has_local_credentials(&self) -> bool {
        self.store.has_credentials()
    }

    pub fn status(&self) -> Result<ApiKeyStatus, CommandCodeError> {
        self.store
            .status()
            .map_err(CommandCodeError::CredentialStorage)
    }

    pub fn save(&self, value: &str) -> Result<(), CommandCodeError> {
        self.store.save(value).map_err(|error| {
            if value.trim().is_empty() {
                CommandCodeError::MissingKey
            } else {
                crate::app_warn!("auth:commandcode", "system credential store write failed");
                CommandCodeError::CredentialStorage(error)
            }
        })
    }

    pub fn delete(&self) -> Result<(), CommandCodeError> {
        self.store.delete().map_err(|error| {
            crate::app_warn!("auth:commandcode", "system credential store delete failed");
            CommandCodeError::CredentialStorage(error)
        })
    }
}

impl Default for CommandCodeAuthStore {
    fn default() -> Self {
        Self::new()
    }
}
