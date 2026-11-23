//! The system credential store: the macOS Keychain, Windows Credential
//! Manager or the Secret Service. Never a file fallback, and no error carries
//! the provider's details.
//!
//! One Windows credential holds at most 1,280 UTF-16 code units, so on Windows
//! a longer secret is split across several credentials (the private `split`
//! module describes how). A secret that fits is still one credential under
//! the account's name, as earlier builds wrote it. The other stores have no
//! such limit and keep every secret as one item, unchanged.

mod split;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use crate::Secret;
use crate::vault::lock;
use split::Limits;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The credential store is unavailable. Unlock your system keychain and try again.")]
pub struct KeychainError;

/// Where one stored item holds too little for a long secret, how much it holds.
const LIMITS: Option<Limits> = if cfg!(windows) {
    Some(Limits::WINDOWS)
} else {
    None
};

/// Serialises split loads, saves and deletes in this process, so a load
/// never reads a manifest whose chunks a save is deleting.
static SPLIT: Mutex<()> = Mutex::new(());

/// The secrets of one service name, one per account.
pub struct Keychain {
    service: String,
}

impl Keychain {
    /// Use a stable product/channel namespace to keep independent apps separate.
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    pub fn load(&self, account: &str) -> Result<Option<Secret>, KeychainError> {
        let _split = serialise();
        split::load(&System(&self.service), LIMITS, account).map(|value| value.map(Secret::new))
    }

    pub fn save(&self, account: &str, secret: &Secret) -> Result<(), KeychainError> {
        let _split = serialise();
        split::save(&System(&self.service), LIMITS, account, secret.expose())
    }

    /// Deletes the account's secret; a missing one is not an error.
    pub fn delete(&self, account: &str) -> Result<(), KeychainError> {
        let _split = serialise();
        split::delete(&System(&self.service), LIMITS, account)
    }
}

/// The split lock, where secrets are split.
fn serialise() -> Option<MutexGuard<'static, ()>> {
    LIMITS.map(|_| lock(&SPLIT))
}

/// What one stored item holds: text, as [`Store::set_text`] writes it, or
/// bytes that are not text, as only [`Store::set_bytes`] writes them.
enum Item {
    Text(String),
    Bytes(Vec<u8>),
}

/// One stored item per account name.
trait Store {
    fn get(&self, account: &str) -> Result<Option<Item>, KeychainError>;
    fn set_text(&self, account: &str, value: &str) -> Result<(), KeychainError>;
    fn set_bytes(&self, account: &str, value: &[u8]) -> Result<(), KeychainError>;
    /// Deletes the account's item; a missing one is not an error.
    fn delete(&self, account: &str) -> Result<(), KeychainError>;
    /// Every account name that starts with `prefix`, compared as the store
    /// compares names.
    fn list(&self, prefix: &str) -> Result<Vec<String>, KeychainError>;
}

/// The system credential store, under one service name.
struct System<'a>(&'a str);

impl Store for System<'_> {
    fn get(&self, account: &str) -> Result<Option<Item>, KeychainError> {
        item(entry(self.0, account)?.get_password())
    }

    fn set_text(&self, account: &str, value: &str) -> Result<(), KeychainError> {
        entry(self.0, account)?
            .set_password(value)
            .map_err(|_| KeychainError)
    }

    fn set_bytes(&self, account: &str, value: &[u8]) -> Result<(), KeychainError> {
        entry(self.0, account)?
            .set_secret(value)
            .map_err(|_| KeychainError)
    }

    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        optional(entry(self.0, account)?.delete_credential()).map(|_| ())
    }

    /// Windows Credential Manager only, the one store whose secrets are
    /// split: its search matches a pattern against `<account>.<service>`.
    fn list(&self, prefix: &str) -> Result<Vec<String>, KeychainError> {
        keyring::v1::Entry::store_status()
            .as_ref()
            .map_err(|_| KeychainError)?;
        let pattern = target_pattern(prefix, self.0);
        let found = keyring_core::Entry::search(&HashMap::from([("pattern", pattern.as_str())]))
            .map_err(|_| KeychainError)?;
        found
            .iter()
            .map(|entry| account_of(entry.get_attributes(), self.0))
            .collect()
    }
}

fn entry(service: &str, account: &str) -> Result<keyring::v1::Entry, KeychainError> {
    keyring::v1::Entry::new(service, account).map_err(|_| KeychainError)
}

/// A read item: text, or bytes the store could not read as text.
fn item(result: keyring::v1::Result<String>) -> Result<Option<Item>, KeychainError> {
    match result {
        Ok(text) => Ok(Some(Item::Text(text))),
        Err(keyring::v1::Error::BadEncoding(bytes)) => Ok(Some(Item::Bytes(bytes))),
        Err(keyring::v1::Error::NoEntry) => Ok(None),
        Err(_) => Err(KeychainError),
    }
}

fn optional<T>(result: keyring::v1::Result<T>) -> Result<Option<T>, KeychainError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(keyring::v1::Error::NoEntry) => Ok(None),
        Err(_) => Err(KeychainError),
    }
}

/// Matches the target names `<account>.<service>` of accounts that start
/// with `prefix`, ignoring case as Windows does.
fn target_pattern(prefix: &str, service: &str) -> String {
    format!(
        "(?i)^{}.*\\.{}$",
        regex::escape(prefix),
        regex::escape(service)
    )
}

/// The account of a credential found by [`target_pattern`]: its target name
/// without `.<service>`.
fn account_of(
    attributes: keyring::v1::Result<HashMap<String, String>>,
    service: &str,
) -> Result<String, KeychainError> {
    let attributes = attributes.map_err(|_| KeychainError)?;
    let target = attributes.get("target_name").ok_or(KeychainError)?;
    let end = target.len().checked_sub(service.len() + 1);
    end.and_then(|end| target.get(..end))
        .map(str::to_owned)
        .ok_or(KeychainError)
}

#[cfg(test)]
mod tests;
