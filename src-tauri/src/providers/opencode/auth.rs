use std::{fs, io::Read, path::PathBuf};

use serde_json::Value;
use zeroize::{Zeroize, Zeroizing};

use crate::{models::ApiKeyStatus, providers::api_key::ApiKeyStore};

use super::OpenCodeError;

const CONFIG_PATHS: &[&str] = &["~/.config/quota01/opencode.json"];
const ENVIRONMENT_NAMES: &[&str] = &["OPENCODE_GO_API_KEY"];
const MAX_AUTH_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct OpenCodeAuthStore {
    store: ApiKeyStore,
    data_directory: PathBuf,
}

impl OpenCodeAuthStore {
    pub fn new() -> Self {
        Self {
            store: ApiKeyStore::new_with_sources("opencode", ENVIRONMENT_NAMES, CONFIG_PATHS),
            data_directory: data_directory(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(store: ApiKeyStore, data_directory: PathBuf) -> Self {
        Self {
            store,
            data_directory,
        }
    }

    /// The app-configured key (vault / environment / config file) wins; otherwise we fall back to
    /// the Go key that the opencode CLI wrote into its own `auth.json` when you signed in to Go.
    pub fn load(&self) -> Result<Option<Zeroizing<String>>, OpenCodeError> {
        match self.store.load() {
            Ok(Some(secret)) => Ok(Some(Zeroizing::new(secret.as_str().to_owned()))),
            Ok(None) => Ok(self.go_key_from_auth_json()),
            // An unreadable system credential store is only fatal when no other source can supply
            // the key; a CLI sign-in still works.
            Err(error) => match self.go_key_from_auth_json() {
                Some(key) => {
                    crate::app_warn!(
                        "auth:opencode",
                        "system credential store unavailable; using the opencode CLI key ({error})"
                    );
                    Ok(Some(key))
                }
                None => Err(OpenCodeError::CredentialStorage),
            },
        }
    }

    pub fn has_local_credentials(&self) -> bool {
        self.store.has_credentials() || self.go_key_from_auth_json().is_some()
    }

    /// Reports where the active key comes from, so Customize can name the source and still offer an
    /// override. A key found in opencode's own `auth.json` is reported as `FromCliSignIn` rather
    /// than as an app-configured source.
    pub fn status(&self) -> Result<ApiKeyStatus, OpenCodeError> {
        match self.store.status() {
            Ok(ApiKeyStatus::NotSet) => Ok(self.cli_sign_in_status()),
            Ok(status) => Ok(status),
            Err(_) => match self.cli_sign_in_status() {
                ApiKeyStatus::NotSet => Err(OpenCodeError::CredentialStorage),
                status => Ok(status),
            },
        }
    }

    pub fn save(&self, value: &str) -> Result<(), OpenCodeError> {
        self.store.save(value).map_err(|_| {
            if value.trim().is_empty() {
                OpenCodeError::MissingKey
            } else {
                OpenCodeError::CredentialStorage
            }
        })
    }

    pub fn delete(&self) -> Result<(), OpenCodeError> {
        self.store
            .delete()
            .map_err(|_| OpenCodeError::CredentialStorage)
    }

    fn cli_sign_in_status(&self) -> ApiKeyStatus {
        if self.go_key_from_auth_json().is_some() {
            ApiKeyStatus::FromCliSignIn
        } else {
            ApiKeyStatus::NotSet
        }
    }

    fn go_key_from_auth_json(&self) -> Option<Zeroizing<String>> {
        let path = self.data_directory.join("auth.json");
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(_) => return None,
        };
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() || metadata.len() > MAX_AUTH_FILE_BYTES {
            return None;
        }
        let mut content = Zeroizing::new(String::with_capacity(metadata.len() as usize));
        file.take(MAX_AUTH_FILE_BYTES + 1)
            .read_to_string(&mut content)
            .ok()?;
        if content.len() as u64 > MAX_AUTH_FILE_BYTES {
            return None;
        }
        let mut value = serde_json::from_str::<Value>(content.as_str()).ok()?;
        let entry = value
            .as_object_mut()?
            .get_mut("opencode-go")
            .and_then(Value::as_object_mut)?;
        let trimmed = entry
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())?;
        let key = Zeroizing::new(trimmed.to_owned());
        if let Some(Value::String(stored)) = entry.get_mut("key") {
            stored.zeroize();
        }
        Some(key)
    }
}

fn data_directory() -> PathBuf {
    if let Some(configured) = std::env::var("OPENCODE_DATA_DIR").ok().and_then(non_empty) {
        return expand_home(&configured);
    }
    if let Some(xdg) = std::env::var("XDG_DATA_HOME").ok().and_then(non_empty) {
        return expand_home(&xdg).join("opencode");
    }
    home_directory()
        .join(".local")
        .join("share")
        .join("opencode")
}

fn home_directory() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return home_directory();
    }
    if let Some(relative) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        return home_directory().join(relative);
    }
    PathBuf::from(value)
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

impl Default for OpenCodeAuthStore {
    fn default() -> Self {
        Self::new()
    }
}
