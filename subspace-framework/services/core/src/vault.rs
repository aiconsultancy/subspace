//! Where Subspace keeps its saved secrets. Release builds use the system
//! keychain ([`Keychain`]). Debug builds use an owner-only JSON file beside
//! the state file instead, unless [`USE_KEYCHAIN_ENV`] is set: a debug build
//! is ad hoc signed, so each rebuild is a new app to the keychain, which
//! asks for access again or, when the running build no longer matches its
//! bundle on disk, refuses without asking. Capture and test builds never
//! touch the real keychain either.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::keychain::Keychain;
use crate::{Secret, state};

/// Debug builds only: set to keep secrets in the system keychain, as a
/// release build does.
pub const USE_KEYCHAIN_ENV: &str = "SUBSPACE_DEVELOPMENT_USE_KEYCHAIN";

/// Serializes every read-modify-write of a vault file.
static WRITES: Mutex<()> = Mutex::new(());

/// The vault could not read or save a secret. No provider details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultError;

/// Secrets by account name.
pub trait Vault: Send + Sync {
    fn load(&self, account: &str) -> Result<Option<Secret>, VaultError>;
    fn save(&self, account: &str, secret: &Secret) -> Result<(), VaultError>;
    fn delete(&self, account: &str) -> Result<(), VaultError>;
}

/// The build's vault: the keychain under `service`, or in a debug build the
/// owner-only `file_name` beside the state file.
pub fn system(service: impl Into<String>, file_name: &str) -> Box<dyn Vault> {
    let use_file = cfg!(debug_assertions) && std::env::var_os(USE_KEYCHAIN_ENV).is_none();
    match use_file {
        true => Box::new(FileVault(state::sibling(file_name))),
        false => Box::new(KeychainVault(Keychain::new(service))),
    }
}

/// An owner-only JSON file of secrets, as debug builds and tests use.
pub fn file(path: PathBuf) -> Box<dyn Vault> {
    Box::new(FileVault(Some(path)))
}

/// Kept in memory for this run only, as tests need.
pub fn memory() -> Box<dyn Vault> {
    Box::new(MemoryVault::default())
}

struct KeychainVault(Keychain);

impl Vault for KeychainVault {
    fn load(&self, account: &str) -> Result<Option<Secret>, VaultError> {
        self.0.load(account).map_err(|_| VaultError)
    }

    fn save(&self, account: &str, secret: &Secret) -> Result<(), VaultError> {
        self.0.save(account, secret).map_err(|_| VaultError)
    }

    fn delete(&self, account: &str) -> Result<(), VaultError> {
        self.0.delete(account).map_err(|_| VaultError)
    }
}

/// An owner-only JSON object of account names and secrets.
struct FileVault(Option<PathBuf>);

impl Vault for FileVault {
    fn load(&self, account: &str) -> Result<Option<Secret>, VaultError> {
        let secrets: BTreeMap<String, String> = read_json(self.0.as_deref());
        Ok(secrets.get(account).map(Secret::new))
    }

    fn save(&self, account: &str, secret: &Secret) -> Result<(), VaultError> {
        let _writing = lock(&WRITES);
        let mut secrets: BTreeMap<String, String> = read_json(self.0.as_deref());
        secrets.insert(account.to_owned(), secret.expose().to_owned());
        write_json(self.0.as_deref(), &secrets)
    }

    fn delete(&self, account: &str) -> Result<(), VaultError> {
        let _writing = lock(&WRITES);
        let mut secrets: BTreeMap<String, String> = read_json(self.0.as_deref());
        secrets.remove(account);
        write_json(self.0.as_deref(), &secrets)
    }
}

#[derive(Default)]
struct MemoryVault(Mutex<BTreeMap<String, Secret>>);

impl Vault for MemoryVault {
    fn load(&self, account: &str) -> Result<Option<Secret>, VaultError> {
        Ok(lock(&self.0).get(account).cloned())
    }

    fn save(&self, account: &str, secret: &Secret) -> Result<(), VaultError> {
        lock(&self.0).insert(account.to_owned(), secret.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), VaultError> {
        lock(&self.0).remove(account);
        Ok(())
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The JSON in `file`; a missing or unreadable file reads as the default.
pub(crate) fn read_json<T: DeserializeOwned + Default>(file: Option<&Path>) -> T {
    file.and_then(|file| fs::read(file).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes `value` to `file` atomically, readable by the owner only.
pub(crate) fn write_json<T: Serialize>(file: Option<&Path>, value: &T) -> Result<(), VaultError> {
    let file = file.ok_or(VaultError)?;
    let text = serde_json::to_string_pretty(value).map_err(|_| VaultError)?;
    state::write_atomic(file, &text).map_err(|_| VaultError)
}

#[cfg(test)]
mod tests {
    use super::{file, memory};
    use crate::Secret;

    #[test]
    fn a_file_vault_is_owner_only_and_forgets_deleted_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("secrets.json");
        let vault = file(path.clone());
        vault
            .save("profile", &Secret::new("fixture-value"))
            .unwrap();
        let reopened = file(path.clone());
        assert_eq!(
            reopened.load("profile").unwrap(),
            Some(Secret::new("fixture-value"))
        );
        assert!(crate::private_file::is_owner_only(&path).unwrap());
        reopened.delete("profile").unwrap();
        assert_eq!(reopened.load("profile").unwrap(), None);
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("fixture-value")
        );
    }

    #[test]
    fn a_memory_vault_keeps_secrets_by_account() {
        let vault = memory();
        vault.save("one", &Secret::new("a")).unwrap();
        assert_eq!(vault.load("one").unwrap(), Some(Secret::new("a")));
        assert_eq!(vault.load("two").unwrap(), None);
        vault.delete("one").unwrap();
        assert_eq!(vault.load("one").unwrap(), None);
    }
}
