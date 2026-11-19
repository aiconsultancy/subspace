//! GitHub accounts signed in through Subspace (credential step 3): the
//! logins in `github-accounts.json` beside the state file, and each
//! account's token in the build's [`vault`]: the system keychain under
//! `<data directory>.github`, or in a debug build the owner-only
//! `github-tokens.json` beside the state file. Logins are not secret; a
//! token never leaves its vault except to authorize a request.

#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::hosts_yml::is_login;
use crate::vault::{self, Vault, VaultError, lock, read_json, write_json};
use crate::{Secret, state};

/// The signed-in logins, beside the state file. Not secret.
pub const LIST_FILE: &str = "github-accounts.json";
/// Debug builds only: the owner-only token file beside the state file.
pub const TOKEN_FILE: &str = "github-tokens.json";
/// Serializes every read-modify-write of the list.
static WRITES: Mutex<()> = Mutex::new(());

/// One signed-in login, spelled as GitHub spells it. `expired` once GitHub
/// rejected its token, until the account signs in again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedInAccount {
    pub login: String,
    #[serde(default)]
    pub expired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "Subspace cannot read or save its GitHub sign-in. Unlock your system keychain and try again."
)]
pub struct StoreError;

/// Where the list of signed-in logins is kept.
enum List {
    File(Option<PathBuf>),
    Memory(Mutex<Vec<SignedInAccount>>),
}

impl List {
    fn read(&self) -> Vec<SignedInAccount> {
        match self {
            Self::File(file) => read_json(file.as_deref()),
            Self::Memory(accounts) => lock(accounts).clone(),
        }
    }

    fn write(&self, accounts: Vec<SignedInAccount>) -> Result<(), StoreError> {
        match self {
            Self::File(file) => Ok(write_json(file.as_deref(), &accounts)?),
            Self::Memory(saved) => {
                *lock(saved) = accounts;
                Ok(())
            }
        }
    }
}

/// The signed-in accounts and their tokens.
pub struct SignedInAccounts {
    list: List,
    vault: Box<dyn Vault>,
}

impl From<VaultError> for StoreError {
    fn from(_: VaultError) -> Self {
        StoreError
    }
}

impl Default for SignedInAccounts {
    fn default() -> Self {
        Self::memory()
    }
}

impl SignedInAccounts {
    /// The list beside the state file, and the build's vault.
    pub fn system() -> Self {
        Self {
            list: List::File(state::sibling(LIST_FILE)),
            vault: vault::system(format!("{}.github", state::directory_name()), TOKEN_FILE),
        }
    }

    /// Kept in memory for this run only, as tests need.
    pub fn memory() -> Self {
        Self {
            list: List::Memory(Mutex::default()),
            vault: vault::memory(),
        }
    }

    /// The list and the debug build's token file in `directory`.
    #[cfg(test)]
    fn in_directory(directory: &Path) -> Self {
        Self {
            list: List::File(Some(directory.join(LIST_FILE))),
            vault: vault::file(directory.join(TOKEN_FILE)),
        }
    }

    pub fn accounts(&self) -> Vec<SignedInAccount> {
        self.list.read()
    }

    /// The signed-in account `login`, whatever its case.
    pub fn find(&self, login: &str) -> Option<SignedInAccount> {
        self.accounts()
            .into_iter()
            .find(|account| account.login.eq_ignore_ascii_case(login))
    }

    /// The token of `login` while GitHub still accepts it.
    pub fn token(&self, login: &str) -> Result<Option<Secret>, StoreError> {
        match self.find(login) {
            Some(account) if !account.expired => Ok(self.vault.load(&key(login))?),
            _ => Ok(None),
        }
    }

    /// Keeps `token` for `login`, replacing an earlier sign-in.
    pub fn save(&self, login: &str, token: &Secret) -> Result<(), StoreError> {
        if !is_login(login) {
            return Err(StoreError);
        }
        self.vault.save(&key(login), token)?;
        self.update(|accounts| signed_in(accounts, login))
    }

    /// Forgets `login` and gives back its token, so it can be revoked.
    pub fn remove(&self, login: &str) -> Result<Option<Secret>, StoreError> {
        let token = self.vault.load(&key(login))?;
        self.vault.delete(&key(login))?;
        self.update(|accounts| accounts.retain(|account| !same(&account.login, login)))?;
        Ok(token)
    }

    /// GitHub rejected `rejected`: marks `login` as needing sign-in again,
    /// unless it has signed in again since, with another token.
    pub fn reject(&self, login: &str, rejected: &Secret) -> Result<(), StoreError> {
        if self.vault.load(&key(login))?.as_ref() != Some(rejected) {
            return Ok(());
        }
        self.update(|accounts| expire(accounts, login))
    }

    /// `saved` when it is a usable signed-in account, else the first one.
    pub fn preferred(&self, saved: Option<&str>) -> Option<String> {
        preferred(&self.accounts(), saved)
    }

    fn update(&self, change: impl FnOnce(&mut Vec<SignedInAccount>)) -> Result<(), StoreError> {
        let _writing = lock(&WRITES);
        let mut accounts = self.list.read();
        change(&mut accounts);
        self.list.write(accounts)
    }
}

fn key(login: &str) -> String {
    login.to_ascii_lowercase()
}

fn same(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn signed_in(accounts: &mut Vec<SignedInAccount>, login: &str) {
    accounts.retain(|account| !same(&account.login, login));
    accounts.push(SignedInAccount {
        login: login.to_owned(),
        expired: false,
    });
}

fn expire(accounts: &mut [SignedInAccount], login: &str) {
    for account in accounts
        .iter_mut()
        .filter(|account| same(&account.login, login))
    {
        account.expired = true;
    }
}

fn preferred(accounts: &[SignedInAccount], saved: Option<&str>) -> Option<String> {
    let usable = || accounts.iter().filter(|account| !account.expired);
    saved
        .and_then(|saved| usable().find(|account| same(&account.login, saved)))
        .or_else(|| usable().next())
        .map(|account| account.login.clone())
}

#[cfg(test)]
mod tests {
    use super::{LIST_FILE, SignedInAccount, SignedInAccounts, StoreError, TOKEN_FILE};
    use crate::Secret;

    fn token(value: &str) -> Secret {
        Secret::new(value)
    }

    #[test]
    fn a_saved_account_gives_its_token_whatever_the_case() {
        let accounts = SignedInAccounts::memory();
        accounts.save("Octo-Dev", &token("first")).unwrap();
        assert_eq!(accounts.token("octo-dev").unwrap(), Some(token("first")));
        accounts.save("octo-dev", &token("second")).unwrap();
        assert_eq!(
            accounts.accounts(),
            [SignedInAccount {
                login: "octo-dev".into(),
                expired: false
            }]
        );
        assert_eq!(accounts.token("OCTO-DEV").unwrap(), Some(token("second")));
        assert_eq!(accounts.token("someone-else").unwrap(), None);
        assert_eq!(accounts.save("--help", &token("bad")), Err(StoreError));
    }

    #[test]
    fn a_rejected_token_needs_sign_in_again_unless_it_was_replaced() {
        let accounts = SignedInAccounts::memory();
        accounts.save("octo-dev", &token("old")).unwrap();
        accounts.save("octo-dev", &token("new")).unwrap();
        accounts.reject("octo-dev", &token("old")).unwrap();
        assert_eq!(accounts.token("octo-dev").unwrap(), Some(token("new")));
        accounts.reject("octo-dev", &token("new")).unwrap();
        assert!(accounts.find("octo-dev").unwrap().expired);
        assert_eq!(accounts.token("octo-dev").unwrap(), None);
        accounts.save("octo-dev", &token("again")).unwrap();
        assert!(!accounts.find("octo-dev").unwrap().expired);
    }

    #[test]
    fn removing_an_account_gives_back_its_token_for_revocation() {
        let accounts = SignedInAccounts::memory();
        accounts.save("octo-dev", &token("value")).unwrap();
        accounts.save("mona", &token("other")).unwrap();
        assert_eq!(accounts.remove("OCTO-dev").unwrap(), Some(token("value")));
        assert_eq!(accounts.find("octo-dev"), None);
        assert_eq!(accounts.remove("octo-dev").unwrap(), None);
        assert_eq!(accounts.token("mona").unwrap(), Some(token("other")));
    }

    #[test]
    fn the_preferred_account_is_the_saved_one_while_it_still_works() {
        let accounts = SignedInAccounts::memory();
        assert_eq!(accounts.preferred(Some("mona")), None);
        accounts.save("octo-dev", &token("one")).unwrap();
        accounts.save("mona", &token("two")).unwrap();
        assert_eq!(accounts.preferred(Some("MONA")).as_deref(), Some("mona"));
        assert_eq!(accounts.preferred(None).as_deref(), Some("octo-dev"));
        accounts.reject("mona", &token("two")).unwrap();
        assert_eq!(
            accounts.preferred(Some("mona")).as_deref(),
            Some("octo-dev")
        );
    }

    #[test]
    fn the_file_store_is_owner_only_and_keeps_tokens_out_of_the_list() {
        let directory = tempfile::tempdir().unwrap();
        let accounts = SignedInAccounts::in_directory(directory.path());
        accounts.save("octo-dev", &token("fixture-value")).unwrap();
        let reopened = SignedInAccounts::in_directory(directory.path());
        assert_eq!(
            reopened.token("octo-dev").unwrap(),
            Some(token("fixture-value"))
        );
        for name in [LIST_FILE, TOKEN_FILE] {
            let file = directory.path().join(name);
            assert!(crate::private_file::is_owner_only(&file).unwrap(), "{name}");
        }
        let list = std::fs::read_to_string(directory.path().join(LIST_FILE)).unwrap();
        assert!(list.contains("octo-dev") && !list.contains("fixture-value"));
        reopened.remove("octo-dev").unwrap();
        let tokens = std::fs::read_to_string(directory.path().join(TOKEN_FILE)).unwrap();
        assert!(!tokens.contains("fixture-value"));
    }
}
