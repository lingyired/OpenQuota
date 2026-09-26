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

#[cfg(any(target_os = "windows", target_os = "linux"))]
use super::credential_store::{delete_owned_password, read_owned_password, write_owned_password};

#[cfg(any(target_os = "windows", target_os = "linux"))]
const VAULT_KEY_SERVICE: &str = "com.lingyi.quota01.credentials";
#[cfg(any(target_os = "windows", target_os = "linux"))]
const VAULT_KEY_ACCOUNT: &str = "vault-key";
const VAULT_AAD: &[u8] = b"Quota01 credential vault v1";
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

pub fn reset() -> Result<(), String> {
    global()?.reset()
}

fn global() -> Result<&'static Arc<CredentialVault>, String> {
    VAULT
        .get()
        .ok_or_else(|| "The credential vault is unavailable.".to_owned())
}

trait VaultKeyStore: Send + Sync {
    fn read(&self) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, value: &[u8]) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
struct SystemVaultKeyStore;

#[cfg(any(target_os = "windows", target_os = "linux"))]
impl VaultKeyStore for SystemVaultKeyStore {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        read_owned_password(VAULT_KEY_SERVICE, VAULT_KEY_ACCOUNT)
    }

    fn write(&self, value: &[u8]) -> Result<(), String> {
        write_owned_password(VAULT_KEY_SERVICE, VAULT_KEY_ACCOUNT, value)
    }

    fn delete(&self) -> Result<(), String> {
        delete_owned_password(VAULT_KEY_SERVICE, VAULT_KEY_ACCOUNT)
    }
}

#[cfg(any(target_os = "macos", test))]
struct FileVaultKeyStore {
    key_path: PathBuf,
    directory: PathBuf,
}

#[cfg(any(target_os = "macos", test))]
impl FileVaultKeyStore {
    fn new(directory: PathBuf) -> Self {
        Self {
            key_path: directory.join("credentials.key"),
            directory,
        }
    }

    fn ensure_directory(&self) -> Result<(), String> {
        fs::create_dir_all(&self.directory)
            .map_err(|_| "The credential vault directory could not be created.".to_owned())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700)).map_err(
                |_| "The credential vault directory permissions could not be set.".to_owned(),
            )?;
            let permissions = fs::symlink_metadata(&self.directory)
                .map_err(|_| {
                    "The credential vault directory permissions could not be verified.".to_owned()
                })?
                .permissions()
                .mode()
                & 0o7777;
            if permissions != 0o700 {
                return Err("The credential vault directory permissions are invalid.".to_owned());
            }
        }
        Ok(())
    }

    fn read_validated(&self) -> Result<Option<Vec<u8>>, String> {
        let metadata = match fs::symlink_metadata(&self.key_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("The credential vault key could not be read.".to_owned()),
        };
        if !metadata.file_type().is_file() {
            return Err("The credential vault key is not a regular file.".to_owned());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o7777 != 0o600 {
                return Err("The credential vault key permissions are invalid.".to_owned());
            }
        }
        let value = fs::read(&self.key_path)
            .map_err(|_| "The credential vault key could not be read.".to_owned())?;
        if value.len() != KEY_LEN {
            return Err("The credential vault key is invalid.".to_owned());
        }
        Ok(Some(value))
    }
}

#[cfg(any(target_os = "macos", test))]
impl VaultKeyStore for FileVaultKeyStore {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        self.ensure_directory()?;
        self.read_validated()
    }

    fn write(&self, value: &[u8]) -> Result<(), String> {
        if value.len() != KEY_LEN {
            return Err("The credential vault key is invalid.".to_owned());
        }
        self.ensure_directory()?;
        if self.read_validated()?.is_some() {
            return Err("The credential vault key already exists.".to_owned());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)
            .map_err(|_| "The credential vault key could not be written.".to_owned())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "The credential vault key permissions could not be set.".to_owned())?;
        }
        temporary
            .write_all(value)
            .map_err(|_| "The credential vault key could not be written.".to_owned())?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| "The credential vault key could not be synced.".to_owned())?;
        match temporary.persist_noclobber(&self.key_path) {
            Ok(file) => {
                file.sync_all()
                    .map_err(|_| "The credential vault key could not be synced.".to_owned())?;
                if let Ok(directory) = fs::File::open(&self.directory) {
                    let _ = directory.sync_all();
                }
                Ok(())
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => self
                .read_validated()?
                .map(|_| ())
                .ok_or_else(|| "The credential vault key could not be created.".to_owned()),
            Err(_) => Err("The credential vault key could not be created.".to_owned()),
        }
    }

    fn delete(&self) -> Result<(), String> {
        match fs::remove_file(&self.key_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("The credential vault key could not be removed.".to_owned()),
        }
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
        #[cfg(target_os = "macos")]
        let key_store: Arc<dyn VaultKeyStore> = Arc::new(FileVaultKeyStore::new(
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
        ));
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let key_store: Arc<dyn VaultKeyStore> = Arc::new(SystemVaultKeyStore);
        Self::with_backends(path, key_store)
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

    fn reset(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The credential vault is unavailable.".to_owned())?;
        match fs::remove_file(&self.path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("The credential vault could not be reset.".to_owned()),
        }
        self.key_store.delete()?;
        *state = VaultState::default();
        Ok(())
    }

    fn ensure_loaded(&self, state: &mut VaultState) -> Result<(), String> {
        if state.loaded {
            return Ok(());
        }

        let key =
            match self.key_store.read()? {
                Some(key) if key.len() == KEY_LEN => Zeroizing::new(key),
                Some(_) => return Err("The credential vault key is invalid.".to_owned()),
                None if self.path.exists() => {
                    return Err("The credential vault key is unavailable.".to_owned());
                }
                None => {
                    let mut key = Zeroizing::new(vec![0_u8; KEY_LEN]);
                    rng().fill_bytes(key.as_mut_slice());
                    self.key_store.write(key.as_slice())?;
                    Zeroizing::new(self.key_store.read()?.ok_or_else(|| {
                        "The credential vault key could not be created.".to_owned()
                    })?)
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

    use super::{CredentialVault, FileVaultKeyStore, VaultKeyStore, KEY_LEN};

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
            *self.value.lock().unwrap() = Some(vec![0x42; value.len()]);
            Ok(())
        }

        fn delete(&self) -> Result<(), String> {
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }

    #[cfg(unix)]
    #[test]
    fn file_key_store_creates_private_directory_and_stable_32_byte_key() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let store = FileVaultKeyStore::new(app_data.clone());

        assert_eq!(store.read().unwrap(), None);
        let key = vec![0x5a; KEY_LEN];
        store.write(&key).unwrap();

        let key_path = app_data.join("credentials.key");
        assert_eq!(fs::read(&key_path).unwrap(), key);
        assert_eq!(store.read().unwrap().unwrap().len(), 32);
        assert_eq!(
            fs::metadata(&key_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&app_data).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(store.read().unwrap(), Some(key));
    }

    #[cfg(unix)]
    #[test]
    fn file_key_store_rejects_tampered_permissions_without_overwriting_key() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let store = FileVaultKeyStore::new(directory.path().join("app-data"));
        store.write(&[0x31; KEY_LEN]).unwrap();
        let key_path = directory.path().join("app-data/credentials.key");
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o640)).unwrap();

        assert!(store.read().is_err());
        assert_eq!(fs::read(&key_path).unwrap(), vec![0x31; KEY_LEN]);
    }

    #[cfg(unix)]
    #[test]
    fn file_key_store_sets_and_verifies_exact_directory_mode() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        fs::create_dir(&app_data).unwrap();
        fs::set_permissions(&app_data, fs::Permissions::from_mode(0o755)).unwrap();

        FileVaultKeyStore::new(app_data.clone())
            .ensure_directory()
            .unwrap();

        assert_eq!(
            fs::symlink_metadata(app_data).unwrap().permissions().mode() & 0o7777,
            0o700
        );
    }

    #[cfg(unix)]
    #[test]
    fn reopening_vault_restores_private_app_data_directory_mode() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let path = app_data.join("credentials.vault");
        let first = CredentialVault::with_backends(
            path.clone(),
            Arc::new(FileVaultKeyStore::new(app_data.clone())),
        );
        first.write("trae-cn", b"session").unwrap();
        fs::set_permissions(&app_data, fs::Permissions::from_mode(0o755)).unwrap();

        let reopened = CredentialVault::with_backends(
            path,
            Arc::new(FileVaultKeyStore::new(app_data.clone())),
        );
        assert_eq!(reopened.read_bytes("trae-cn").unwrap().unwrap(), b"session");
        assert_eq!(
            fs::metadata(app_data).unwrap().permissions().mode() & 0o7777,
            0o700
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_key_store_rejects_special_permission_bits() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().unwrap();
        let store = FileVaultKeyStore::new(directory.path().join("app-data"));
        store.write(&[0x31; KEY_LEN]).unwrap();
        let key_path = directory.path().join("app-data/credentials.key");
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o2600)).unwrap();

        assert!(store.read().is_err());
        assert_eq!(fs::read(&key_path).unwrap(), vec![0x31; KEY_LEN]);
    }

    #[cfg(unix)]
    #[test]
    fn file_key_store_rejects_wrong_length_and_symlink_key_files() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let store = FileVaultKeyStore::new(app_data.clone());
        fs::create_dir_all(&app_data).unwrap();
        fs::write(app_data.join("credentials.key"), b"short").unwrap();
        assert!(store.read().is_err());

        fs::remove_file(app_data.join("credentials.key")).unwrap();
        let target = directory.path().join("target");
        fs::write(&target, vec![0x11; KEY_LEN]).unwrap();
        symlink(&target, app_data.join("credentials.key")).unwrap();
        assert!(store.read().is_err());
    }

    #[test]
    fn existing_vault_without_key_fails_without_changing_encrypted_bytes() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("credentials.vault");
        let original = b"existing encrypted bytes";
        fs::write(&path, original).unwrap();
        let vault =
            CredentialVault::with_backends(path.clone(), Arc::new(MemoryKeyStore::default()));

        assert!(vault.read_bytes("trae-cn").is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
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
        assert_eq!(keys.reads.load(Ordering::SeqCst), 2);
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
        assert_eq!(keys.reads.load(Ordering::SeqCst), 2);
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

    #[test]
    fn repeated_initialization_reads_the_same_file_key_and_data() {
        let directory = tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let path = app_data.join("credentials.vault");
        let first = CredentialVault::with_backends(
            path.clone(),
            Arc::new(FileVaultKeyStore::new(app_data.clone())),
        );
        first.write("trae-cn", b"session").unwrap();
        let key_before = fs::read(app_data.join("credentials.key")).unwrap();

        let reopened = CredentialVault::with_backends(
            path,
            Arc::new(FileVaultKeyStore::new(app_data.clone())),
        );
        assert_eq!(reopened.read_bytes("trae-cn").unwrap().unwrap(), b"session");
        assert_eq!(
            fs::read(app_data.join("credentials.key")).unwrap(),
            key_before
        );
    }

    #[test]
    fn reset_removes_vault_and_key_then_initializes_empty_vault() {
        let directory = tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let path = app_data.join("credentials.vault");
        let keys = Arc::new(MemoryKeyStore::default());
        let vault = CredentialVault::with_backends(path.clone(), keys.clone());
        vault.write("trae-cn", b"session").unwrap();

        vault.reset().unwrap();
        assert!(!path.exists());
        assert!(keys.value.lock().unwrap().is_none());
        assert!(!vault.contains("trae-cn").unwrap());
        assert!(path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn file_backed_reset_removes_key_and_reinitializes_an_empty_vault() {
        let directory = tempdir().unwrap();
        let app_data = directory.path().join("app-data");
        let path = app_data.join("credentials.vault");
        let make_vault = || {
            CredentialVault::with_backends(
                path.clone(),
                Arc::new(FileVaultKeyStore::new(app_data.clone())),
            )
        };
        let vault = make_vault();
        vault.write("trae-cn", b"session").unwrap();
        let key_path = app_data.join("credentials.key");
        assert!(key_path.exists());

        vault.reset().unwrap();
        assert!(!path.exists());
        assert!(!key_path.exists());

        let reopened = make_vault();
        assert!(!reopened.contains("trae-cn").unwrap());
        assert!(path.exists());
        assert!(key_path.exists());
    }
}
