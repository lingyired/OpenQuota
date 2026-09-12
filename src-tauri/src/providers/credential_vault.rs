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

use super::credential_store::{read_owned_password, write_owned_password};

const VAULT_KEY_SERVICE: &str = "com.lingyi.usage01.credentials";
const VAULT_KEY_ACCOUNT: &str = "vault-key";
const VAULT_AAD: &[u8] = b"Usage01 credential vault v1";
const VAULT_VERSION: u8 = 1;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
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

type VaultEntries = BTreeMap<String, Zeroizing<String>>;
#[derive(Default)]
struct VaultState {
    key: Option<Zeroizing<Vec<u8>>>,
    entries: VaultEntries,
    loaded: bool,
}

struct CredentialVault {
    path: PathBuf,
    key_store: Arc<dyn VaultKeyStore>,
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
        Self::with_backends(path, Arc::new(SystemVaultKeyStore))
    }

    fn with_backends(path: PathBuf, key_store: Arc<dyn VaultKeyStore>) -> Self {
        Self {
            path,
            key_store,
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

        let entries = if self.path.exists() {
            self.decrypt(&key)?
        } else {
            let entries = BTreeMap::new();
            self.write_file(&key, &entries)?;
            entries
        };

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
        fs,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    use tempfile::tempdir;

    use super::{CredentialVault, VaultKeyStore};

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

    #[test]
    fn write_through_cache_avoids_repeated_key_reads() {
        let directory = tempdir().unwrap();
        let keys = Arc::new(MemoryKeyStore::default());
        let vault = CredentialVault::with_backends(
            directory.path().join("credentials.vault"),
            keys.clone(),
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
    fn new_vault_starts_empty_without_touching_legacy_stores() {
        let directory = tempdir().unwrap();
        let keys = Arc::new(MemoryKeyStore::default());
        let vault = CredentialVault::with_backends(
            directory.path().join("credentials.vault"),
            keys.clone(),
        );

        assert!(!vault.contains("trae-cn").unwrap());
        assert!(vault.read_bytes("trae-cn").unwrap().is_none());
        assert_eq!(keys.reads.load(Ordering::SeqCst), 1);
        assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn encrypted_file_cannot_be_tampered_with() {
        let directory = tempdir().unwrap();
        let keys = Arc::new(MemoryKeyStore::default());
        let path = directory.path().join("credentials.vault");
        let vault = CredentialVault::with_backends(path.clone(), keys.clone());
        vault.write("deepseek", b"sk-secret").unwrap();

        let mut contents = fs::read(&path).unwrap();
        let last = contents.last_mut().unwrap();
        *last ^= 1;
        fs::write(&path, contents).unwrap();

        let reopened = CredentialVault::with_backends(path, keys);
        assert!(reopened.read_bytes("deepseek").is_err());
    }
}
