//! One saved Fleet API token per fleet, in the app's secret vault: the
//! system keychain in release builds, an owner-only file beside the state
//! file in debug builds (`fleet-api-tokens.json`), and memory in tests.
//! Calls block on the keychain: run them in the background.
//!
//! Each token is saved with the normalised address it was saved for, as
//! `{"v":1,"endpoint":…,"token":…}`, the Fleet Console's own form, under
//! `fleet-api:<fleet id>`. [`SavedToken::for_address`] hands the token out
//! only for that address: when the address the Subspace API records for the
//! fleet differs, the token is not sent and the person is asked to confirm
//! the address by entering the token again.

use serde::{Deserialize, Serialize};
use subspace_framework::core::Secret;
use subspace_framework::core::state;
use subspace_framework::core::vault::{self, Vault};

use crate::FleetAddress;

/// Debug builds keep the tokens in this owner-only file beside the state
/// file, as other debug-build secrets are kept.
pub const TOKEN_FILE: &str = "fleet-api-tokens.json";
const ACCOUNT_PREFIX: &str = "fleet-api:";
const MAX_FLEET_ID_CHARS: usize = 128;
const MAX_TOKEN_CHARS: usize = 4096;
const VERSION: u8 = 1;

/// The token store could not read or save. No provider details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error(
        "Subspace could not reach its secret store. Unlock your system keychain and try again."
    )]
    Unavailable,
    #[error("Paste a token of 1 to 4096 characters, without spaces or control characters.")]
    InvalidToken,
    #[error("This fleet's ID cannot name a saved token.")]
    InvalidFleet,
}

/// A saved token and the address it was saved for. Deliberately has no
/// `Debug` that could print the token.
#[derive(Clone)]
pub struct SavedToken {
    address: String,
    token: Secret,
}

/// Why a saved token cannot be used for an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenMismatch {
    /// Saved for this other address, normalised.
    SavedFor(String),
    /// Saved in a form this version cannot read the address of.
    Unbound,
}

impl SavedToken {
    /// `token` as saved for `address`.
    pub(crate) fn new(address: &FleetAddress, token: Secret) -> Self {
        Self {
            address: address.as_str().to_owned(),
            token,
        }
    }

    /// The normalised address the token was saved for.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The token, only when it was saved for `address`.
    pub fn for_address(&self, address: &FleetAddress) -> Result<&Secret, TokenMismatch> {
        match FleetAddress::parse(&self.address) {
            Ok(saved) if saved == *address => Ok(&self.token),
            Ok(saved) => Err(TokenMismatch::SavedFor(saved.as_str().to_owned())),
            Err(_) => Err(TokenMismatch::Unbound),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    v: u8,
    endpoint: String,
    token: String,
}

/// The saved tokens.
pub struct TokenStore {
    vault: Box<dyn Vault>,
}

impl TokenStore {
    /// The app's store: the keychain, or in a debug build [`TOKEN_FILE`].
    pub fn system() -> Self {
        Self {
            vault: vault::system(format!("{}.fleet-api", state::directory_name()), TOKEN_FILE),
        }
    }

    /// Tokens kept in `vault`, as a test with a vault of its own needs,
    /// such as a locked keychain. Only for tests (`test-support`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_vault(vault: Box<dyn Vault>) -> Self {
        Self { vault }
    }

    /// Tokens kept in memory for this run only, as tests need.
    pub fn memory() -> Self {
        Self {
            vault: vault::memory(),
        }
    }

    /// Tokens kept in the owner-only file `path`.
    // qual:test_helper
    pub fn file(path: std::path::PathBuf) -> Self {
        Self {
            vault: vault::file(path),
        }
    }

    /// The token saved for fleet `fleet`, if any.
    pub fn load(&self, fleet: &str) -> Result<Option<SavedToken>, StoreError> {
        let account = account(fleet)?;
        let saved = self
            .vault
            .load(&account)
            .map_err(|_| StoreError::Unavailable)?;
        Ok(saved.map(|secret| unpack(&secret)))
    }

    /// Saves `token` for fleet `fleet`, bound to `address`, replacing any
    /// earlier one.
    pub fn save(
        &self,
        fleet: &str,
        address: &FleetAddress,
        token: &Secret,
    ) -> Result<(), StoreError> {
        let account = account(fleet)?;
        let token = clean_token(token)?;
        let stored = Stored {
            v: VERSION,
            endpoint: address.as_str().to_owned(),
            token: token.to_owned(),
        };
        let packed = serde_json::to_string(&stored).map_err(|_| StoreError::Unavailable)?;
        self.vault
            .save(&account, &Secret::new(packed))
            .map_err(|_| StoreError::Unavailable)
    }

    /// Forgets fleet `fleet`'s token; a missing one is not an error.
    pub fn clear(&self, fleet: &str) -> Result<(), StoreError> {
        let account = account(fleet)?;
        self.vault
            .delete(&account)
            .map_err(|_| StoreError::Unavailable)
    }
}

/// The vault account of fleet `fleet`.
fn account(fleet: &str) -> Result<String, StoreError> {
    let valid = !fleet.is_empty()
        && fleet.len() <= MAX_FLEET_ID_CHARS
        && fleet
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(format!("{ACCOUNT_PREFIX}{fleet}"))
    } else {
        Err(StoreError::InvalidFleet)
    }
}

/// A pasted token, trimmed, or why it cannot be one.
pub fn clean_token(token: &Secret) -> Result<&str, StoreError> {
    let token = token.expose().trim();
    let valid = !token.is_empty()
        && token.chars().count() <= MAX_TOKEN_CHARS
        && !token.chars().any(|c| c.is_control() || c.is_whitespace());
    if valid {
        Ok(token)
    } else {
        Err(StoreError::InvalidToken)
    }
}

/// A stored value as a token and its address. Anything else, such as a
/// bare token, is a token without an address, which is never sent.
fn unpack(secret: &Secret) -> SavedToken {
    let stored = serde_json::from_str::<Stored>(secret.expose())
        .ok()
        .filter(|stored| stored.v == VERSION)
        .unwrap_or_default();
    SavedToken {
        address: stored.endpoint,
        token: Secret::new(stored.token),
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::core::Secret;

    use super::{StoreError, TokenMismatch, TokenStore};
    use crate::FleetAddress;

    fn address(text: &str) -> FleetAddress {
        FleetAddress::parse(text).unwrap()
    }

    #[test]
    fn a_token_is_saved_replaced_and_cleared_per_fleet() {
        let store = TokenStore::memory();
        let home = address("https://fleet.example.com");
        assert!(store.load("aurora").unwrap().is_none());
        store
            .save("aurora", &home, &Secret::new(" first \n"))
            .unwrap();
        store
            .save("borealis", &home, &Secret::new("other"))
            .unwrap();
        let saved = store.load("aurora").unwrap().unwrap();
        assert_eq!(saved.for_address(&home).unwrap().expose(), "first");
        store.save("aurora", &home, &Secret::new("second")).unwrap();
        let saved = store.load("aurora").unwrap().unwrap();
        assert_eq!(saved.for_address(&home).unwrap().expose(), "second");
        store.clear("aurora").unwrap();
        assert!(store.load("aurora").unwrap().is_none());
        assert!(store.load("borealis").unwrap().is_some());
        store.clear("aurora").unwrap();
    }

    #[test]
    fn a_token_is_only_handed_out_for_the_address_it_was_saved_for() {
        let store = TokenStore::memory();
        store
            .save(
                "aurora",
                &address("https://Fleet.example.com/"),
                &Secret::new("t"),
            )
            .unwrap();
        let saved = store.load("aurora").unwrap().unwrap();
        assert!(
            saved
                .for_address(&address("https://fleet.example.com"))
                .is_ok()
        );
        assert_eq!(
            saved
                .for_address(&address("https://elsewhere.example.com"))
                .err(),
            Some(TokenMismatch::SavedFor("https://fleet.example.com".into()))
        );
        assert!(
            saved
                .for_address(&address("https://fleet.example.com:8443"))
                .is_err()
        );
    }

    #[test]
    fn a_token_saved_without_its_address_is_never_used() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tokens.json");
        std::fs::write(&path, r#"{"fleet-api:aurora": "bare-token"}"#).unwrap();
        let store = TokenStore::file(path);
        let saved = store.load("aurora").unwrap().unwrap();
        assert_eq!(
            saved
                .for_address(&address("https://fleet.example.com"))
                .err(),
            Some(TokenMismatch::Unbound)
        );
    }

    #[test]
    fn invalid_tokens_and_fleet_ids_are_refused() {
        let store = TokenStore::memory();
        let home = address("https://fleet.example.com");
        for token in ["", "   ", "two words", "line\nbreak"] {
            assert_eq!(
                store.save("aurora", &home, &Secret::new(token)),
                Err(StoreError::InvalidToken),
                "{token:?}"
            );
        }
        let long = "a".repeat(129);
        for fleet in ["", "a/b", "a:b", long.as_str()] {
            assert_eq!(store.load(fleet).err(), Some(StoreError::InvalidFleet));
        }
        assert!(store.save("fleet-1.a_b", &home, &Secret::new("t")).is_ok());
    }

    #[test]
    fn errors_never_carry_the_token() {
        let store = TokenStore::memory();
        let error = store
            .save(
                "a/b",
                &address("https://fleet.example.com"),
                &Secret::new("sekrit"),
            )
            .unwrap_err();
        assert!(!error.to_string().contains("sekrit"));
    }
}
