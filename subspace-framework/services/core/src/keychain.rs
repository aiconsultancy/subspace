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

use crate::Secret;
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
        split::load(&System(&self.service), LIMITS, account).map(|value| value.map(Secret::new))
    }

    pub fn save(&self, account: &str, secret: &Secret) -> Result<(), KeychainError> {
        split::save(&System(&self.service), LIMITS, account, secret.expose())
    }

    /// Deletes the account's secret; a missing one is not an error.
    pub fn delete(&self, account: &str) -> Result<(), KeychainError> {
        split::delete(&System(&self.service), LIMITS, account)
    }
}

/// One stored value per account name.
trait Store {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError>;
    fn set(&self, account: &str, value: &str) -> Result<(), KeychainError>;
    /// Deletes the account's value; a missing one is not an error.
    fn delete(&self, account: &str) -> Result<(), KeychainError>;
}

/// The system credential store, under one service name.
struct System<'a>(&'a str);

impl Store for System<'_> {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError> {
        optional(entry(self.0, account)?.get_password())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), KeychainError> {
        entry(self.0, account)?
            .set_password(value)
            .map_err(|_| KeychainError)
    }

    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        optional(entry(self.0, account)?.delete_credential()).map(|_| ())
    }
}

fn entry(service: &str, account: &str) -> Result<keyring::v1::Entry, KeychainError> {
    keyring::v1::Entry::new(service, account).map_err(|_| KeychainError)
}

fn optional<T>(result: keyring::v1::Result<T>) -> Result<Option<T>, KeychainError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(keyring::v1::Error::NoEntry) => Ok(None),
        Err(_) => Err(KeychainError),
    }
}

#[cfg(test)]
mod tests;
