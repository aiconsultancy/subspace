//! The system credential store: the macOS Keychain, Windows Credential
//! Manager or the Secret Service. Never a file fallback, and no error carries
//! the provider's details.

use crate::Secret;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The credential store is unavailable. Unlock your system keychain and try again.")]
pub struct KeychainError;

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

    fn entry(&self, account: &str) -> Result<keyring::v1::Entry, KeychainError> {
        keyring::v1::Entry::new(&self.service, account).map_err(|_| KeychainError)
    }

    pub fn load(&self, account: &str) -> Result<Option<Secret>, KeychainError> {
        optional(self.entry(account)?.get_password()).map(|value| value.map(Secret::new))
    }

    pub fn save(&self, account: &str, secret: &Secret) -> Result<(), KeychainError> {
        self.entry(account)?
            .set_password(secret.expose())
            .map_err(|_| KeychainError)
    }

    /// Deletes the account's secret; a missing one is not an error.
    pub fn delete(&self, account: &str) -> Result<(), KeychainError> {
        optional(self.entry(account)?.delete_credential()).map(|_| ())
    }
}

fn optional<T>(result: keyring::v1::Result<T>) -> Result<Option<T>, KeychainError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(keyring::v1::Error::NoEntry) => Ok(None),
        Err(_) => Err(KeychainError),
    }
}

#[cfg(test)]
mod tests {
    use super::{KeychainError, optional};

    #[test]
    fn store_errors_never_expose_provider_details() {
        assert_eq!(optional::<()>(Err(keyring::v1::Error::NoEntry)), Ok(None));
        assert_eq!(optional(Ok("fake")), Ok(Some("fake")));
        assert!(!KeychainError.to_string().contains("fake"));
    }
}
