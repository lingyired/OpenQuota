use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use rand::{rng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::credential_store::{delete_owned_password, read_owned_password, write_owned_password};

const VAULT_KEY_SERVICE: &str = "com.lingyi.usage01.credentials";
const VAULT_KEY_ACCOUNT: &str = "vault-key";
const VAULT_AAD: &[u8] = b"Usage01 credential vault v1";
const VAULT_VERSION: u8 = 1;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const LEGACY_SERVICES: &[&str] = &[
    "com.lingyi.usage01.api-key",
    "com.lingyi.openquota01.api-key",
];
const LEGACY_ACCOUNTS: &[&str] = &[
    "openrouter",
    "zai",
    "kimi",
    "minimax",
    "deepseek",
    "trae-cn",
];

static VAULT: OnceLock<Arc<CredentialVault>> = OnceLock::new();

pub fn initialize(path: PathBuf) -> Result<(), String> {
    let vault = Arc::new(CredentialVault::new(path));
    VAULT
        .set(vault)
        .map_err(|_| "The credential vault was already initialized.".to_owned())
}

pub fn read(account: &str) -> Result<Option<Vec<u8>>, String> {
    global()?.read_bytes(account)
}

pub fn contains(account: &str) -> Result<bool, String> {
    global()?.contains(account)
}

pub fn write(account: &str, value: &[u8]) -> Result<(), String> {
    global()?.write(account, value)
}

pub fn delete(account: &str) -> Result<(), String> {
    global()?.delete(account)
}

fn global() -> Result<&'static Arc<CredentialVault>, String> {
    VAULT
        .get()
        .ok_or_else(|| "The credential vault is unavailable.".to_owned())
}

trait VaultKeyStore: Send + Sync {
    fn read(&self) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, value: &[u8]) -> Result<(), String>;
}

struct SystemVaultKeyStore;

impl VaultKeyStore for SystemVaultKeyStore {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        read_owned_password(VAULT_KEY_SERVICE, VAULT_KEY_ACCOUNT)
    }

    fn write(&self, value: &[u8]) -> Result<(), String> {
        write_owned_password(VAULT_KEY_SERVICE, VAULT_KEY_ACCOUNT, value)
    }
}

trait LegacyCredentialStore: Send + Sync {
    fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String>;
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

struct SystemLegacyCredentialStore;

impl LegacyCredentialStore for SystemLegacyCredentialStore {
    fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        read_owned_password(service, account)
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        delete_owned_password(service, account)
    }
}

type VaultEntries = BTreeMap<String, Zeroizing<String>>;
type LegacyMigration = (VaultEntries, Vec<(String, String)>);

#[derive(Default)]
struct VaultState {
    key: Option<Zeroizing<Vec<u8>>>,
    entries: VaultEntries,
    loaded: bool,
}

struct CredentialVault {
    path: PathBuf,
    key_store: Arc<dyn VaultKeyStore>,
    legacy_store: Arc<dyn LegacyCredentialStore>,
    state: Mutex<VaultState>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedVault {
    version: u8,
    nonce: String,
    ciphertext: String,
}

impl CredentialVault {
    fn new(path: PathBuf) -> Self {
        Self::with_backends(
            path,
            Arc::new(SystemVaultKeyStore),
            Arc::new(SystemLegacyCredentialStore),
        )
    }

    fn with_backends(
        path: PathBuf,
        key_store: Arc<dyn VaultKeyStore>,
        legacy_store: Arc<dyn LegacyCredentialStore>,
    ) -> Self {
        Self {
            path,
            key_store,
            legacy_store,
            state: Mutex::new(VaultState::default()),
        }
    }

    fn read_bytes(&self, account: &str) -> Result<Option<Vec<u8>>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The credential vault is unavailable.".to_owned())?;
        self.ensure_loaded(&mut state)?;
        Ok(state
            .entries
            .get(account)
            .map(|value| value.as_bytes().to_vec()))
    }

    fn contains(&self, account: &str) -> Result<bool, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The credential vault is unavailable.".to_owned())?;
        self.ensure_loaded(&mut state)?;
        Ok(state.entries.contains_key(account))
    }

    fn write(&self, account: &str, value: &[u8]) -> Result<(), String> {
        let value = std::str::from_utf8(value)
            .map_err(|_| "The credential has an unsupported encoding.".to_owned())?
            .trim();
        if value.is_empty() {
            return Err("The credential cannot be empty.".to_owned());
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The credential vault is unavailable.".to_owned())?;
        self.ensure_loaded(&mut state)?;
        let previous = state
            .entries
            .insert(account.to_owned(), Zeroizing::new(value.to_owned()));
        if let Err(error) = self.persist(&state) {
            if let Some(previous) = previous {
                state.entries.insert(account.to_owned(), previous);
            } else {
                state.entries.remove(account);
            }
            return Err(error);
        }
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The credential vault is unavailable.".to_owned())?;
        self.ensure_loaded(&mut state)?;
        let previous = state.entries.remove(account);
        if let Err(error) = self.persist(&state) {
            if let Some(previous) = previous {
                state.entries.insert(account.to_owned(), previous);
            }
            return Err(error);
        }
        Ok(())
    }

    fn ensure_loaded(&self, state: &mut VaultState) -> Result<(), String> {
        if state.loaded {
            return Ok(());
        }

        let key = match self.key_store.read()? {
            Some(key) if key.len() == KEY_LEN => Zeroizing::new(key),
            Some(_) => return Err("The credential vault key is invalid.".to_owned()),
            None if self.path.exists() => {
                return Err("The credential vault key is unavailable.".to_owned());
            }
            None => {
                let mut key = Zeroizing::new(vec![0_u8; KEY_LEN]);
                rng().fill_bytes(key.as_mut_slice());
                self.key_store.write(key.as_slice())?;
                key
            }
        };

        let (entries, migrated) = if self.path.exists() {
            (self.decrypt(&key)?, Vec::new())
        } else {
            let (entries, migrated) = self.migrate_legacy()?;
            self.write_file(&key, &entries)?;
            (entries, migrated)
        };
        for (service, account) in migrated {
            if let Err(error) = self.legacy_store.delete(&service, &account) {
                crate::app_warn!(
                    "auth",
                    "legacy credential cleanup for {account} failed: {error}"
                );
            }
        }

        state.key = Some(key);
        state.entries = entries;
        state.loaded = true;
        Ok(())
    }

    fn persist(&self, state: &VaultState) -> Result<(), String> {
        let key = state
            .key
            .as_ref()
            .ok_or_else(|| "The credential vault is locked.".to_owned())?;
        self.write_file(key, &state.entries)
    }

    fn write_file(&self, key: &[u8], entries: &VaultEntries) -> Result<(), String> {
        let plaintext = Zeroizing::new(
            serde_json::to_vec(
                &entries
                    .iter()
                    .map(|(account, value)| (account.as_str(), value.as_str()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .map_err(|_| "The credential vault could not be encoded.".to_owned())?,
        );
        let cipher = ChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| "The credential vault key is invalid.".to_owned())?;
        let mut nonce = [0_u8; NONCE_LEN];
        rng().fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: VAULT_AAD,
                },
            )
            .map_err(|_| "The credential vault could not be encrypted.".to_owned())?;
        let encoded = serde_json::to_vec(&EncryptedVault {
            version: VAULT_VERSION,
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        })
        .map_err(|_| "The credential vault could not be encoded.".to_owned())?;
        atomic_write(&self.path, &encoded)
    }

    fn decrypt(&self, key: &[u8]) -> Result<VaultEntries, String> {
        let encoded = fs::read(&self.path)
            .map_err(|_| "The credential vault could not be read.".to_owned())?;
        let envelope: EncryptedVault = serde_json::from_slice(&encoded)
            .map_err(|_| "The credential vault is invalid.".to_owned())?;
        if envelope.version != VAULT_VERSION {
            return Err("The credential vault version is unsupported.".to_owned());
        }
        let nonce = STANDARD
            .decode(envelope.nonce)
            .map_err(|_| "The credential vault nonce is invalid.".to_owned())?;
        if nonce.len() != NONCE_LEN {
            return Err("The credential vault nonce is invalid.".to_owned());
        }
        let ciphertext = STANDARD
            .decode(envelope.ciphertext)
            .map_err(|_| "The credential vault ciphertext is invalid.".to_owned())?;
        let cipher = ChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| "The credential vault key is invalid.".to_owned())?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &ciphertext,
                        aad: VAULT_AAD,
                    },
                )
                .map_err(|_| "The credential vault could not be decrypted.".to_owned())?,
        );
        let decoded: BTreeMap<String, String> = serde_json::from_slice(plaintext.as_slice())
            .map_err(|_| "The credential vault is invalid.".to_owned())?;
        Ok(decoded
            .into_iter()
            .map(|(account, value)| (account, Zeroizing::new(value)))
            .collect())
    }

    fn migrate_legacy(&self) -> Result<LegacyMigration, String> {
        let mut entries = BTreeMap::new();
        let mut cleanup = Vec::new();
        for account in LEGACY_ACCOUNTS {
            let mut found = false;
            for service in LEGACY_SERVICES {
                let value = self.legacy_store.read(service, account)?;
                let Some(value) = value else {
                    continue;
                };
                cleanup.push(((*service).to_owned(), (*account).to_owned()));
                if found {
                    continue;
                }
                match std::str::from_utf8(&value) {
                    Ok(value) if !value.trim().is_empty() => {
                        entries.insert(
                            (*account).to_owned(),
                            Zeroizing::new(value.trim().to_owned()),
                        );
                        found = true;
                    }
                    _ => crate::app_warn!(
                        "auth",
                        "legacy credential for {account} could not be migrated"
                    ),
                }
            }
        }
        Ok((entries, cleanup))
    }
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "The credential vault path is invalid.".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|_| "The credential vault directory could not be created.".to_owned())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "The credential vault could not be written.".to_owned())?;
    temporary
        .write_all(contents)
        .map_err(|_| "The credential vault could not be written.".to_owned())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| "The credential vault could not be synced.".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| "The credential vault permissions could not be updated.".to_owned())?;
    }
    temporary
        .persist(path)
        .map_err(|error| error.error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    use tempfile::tempdir;

    use super::{CredentialVault, LegacyCredentialStore, VaultKeyStore, LEGACY_SERVICES};

    #[derive(Default)]
    struct MemoryKeyStore {
        value: Mutex<Option<Vec<u8>>>,
        reads: AtomicUsize,
        writes: AtomicUsize,
    }

    impl VaultKeyStore for MemoryKeyStore {
        fn read(&self) -> Result<Option<Vec<u8>>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.value.lock().unwrap().clone())
        }

        fn write(&self, value: &[u8]) -> Result<(), String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            *self.value.lock().unwrap() = Some(value.to_vec());
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryLegacyStore {
        values: Mutex<HashMap<(String, String), Vec<u8>>>,
        deleted: Mutex<Vec<(String, String)>>,
    }

    impl LegacyCredentialStore for MemoryLegacyStore {
        fn read(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .get(&(service.to_owned(), account.to_owned()))
                .cloned())
        }

        fn delete(&self, service: &str, account: &str) -> Result<(), String> {
            self.deleted
                .lock()
                .unwrap()
                .push((service.to_owned(), account.to_owned()));
            self.values
                .lock()
                .unwrap()
                .remove(&(service.to_owned(), account.to_owned()));
            Ok(())
        }
    }

    #[test]
    fn write_through_cache_avoids_repeated_key_reads() {
        let directory = tempdir().unwrap();
        let keys = Arc::new(MemoryKeyStore::default());
        let legacy = Arc::new(MemoryLegacyStore::default());
        let vault = CredentialVault::with_backends(
            directory.path().join("credentials.vault"),
            keys.clone(),
            legacy,
        );

        vault.write("trae-cn", b"session").unwrap();
        assert_eq!(vault.read_bytes("trae-cn").unwrap().unwrap(), b"session");
        vault.write("trae-cn", b"updated-session").unwrap();
        assert_eq!(
            vault.read_bytes("trae-cn").unwrap().unwrap(),
            b"updated-session"
        );
        assert_eq!(
            vault.read_bytes("trae-cn").unwrap().unwrap(),
            b"updated-session"
        );
        assert_eq!(keys.reads.load(Ordering::SeqCst), 1);
        assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn legacy_values_are_migrated_and_removed_after_the_vault_is_written() {
        let directory = tempdir().unwrap();
        let keys = Arc::new(MemoryKeyStore::default());
        let legacy = Arc::new(MemoryLegacyStore::default());
        legacy.values.lock().unwrap().insert(
            (LEGACY_SERVICES[0].to_owned(), "trae-cn".to_owned()),
            b"legacy-session".to_vec(),
        );
        let path = directory.path().join("credentials.vault");
        let vault = CredentialVault::with_backends(path.clone(), keys, legacy.clone());

        assert_eq!(
            vault.read_bytes("trae-cn").unwrap().unwrap(),
            b"legacy-session"
        );
        assert!(path.exists());
        assert!(legacy
            .deleted
            .lock()
            .unwrap()
            .contains(&(LEGACY_SERVICES[0].to_owned(), "trae-cn".to_owned())));
    }

    #[test]
    fn encrypted_file_cannot_be_tampered_with() {
        let directory = tempdir().unwrap();
        let keys = Arc::new(MemoryKeyStore::default());
        let legacy = Arc::new(MemoryLegacyStore::default());
        let path = directory.path().join("credentials.vault");
        let vault = CredentialVault::with_backends(path.clone(), keys.clone(), legacy.clone());
        vault.write("deepseek", b"sk-secret").unwrap();

        let mut contents = fs::read(&path).unwrap();
        let last = contents.last_mut().unwrap();
        *last ^= 1;
        fs::write(&path, contents).unwrap();

        let reopened = CredentialVault::with_backends(path, keys, legacy);
        assert!(reopened.read_bytes("deepseek").is_err());
    }
}
