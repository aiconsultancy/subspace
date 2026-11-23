//! An in-memory store with the Windows limits, and checks on what it holds.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest as _, Sha256};

use crate::keychain::split::{Limits, load, save};
use crate::keychain::{Item, KeychainError, Store};

/// `CRED_MAX_CREDENTIAL_BLOB_SIZE`: the bytes one Windows credential holds.
pub(super) const BLOB_BYTES: usize = 2_560;
/// The same limit for a password, which keyring stores as UTF-16.
pub(super) const ITEM_UNITS: usize = BLOB_BYTES / 2;
/// `CRED_MAX_USERNAME_LENGTH`, which keyring checks in bytes.
pub(super) const ACCOUNT_BYTES: usize = 513;
/// The stored format's piece size, chunk header bound and chunk limit.
pub(super) const PIECE_UNITS: usize = 960;
pub(super) const HEADER_UNITS: usize = 37;
pub(super) const MAX_CHUNKS: usize = 256;
pub(super) const MANIFEST_PREFIX: &[u8] = b"\xffsubspace-keychain/chunks/v2/";
pub(super) const ACCOUNT: &str = "session";
pub(super) const WINDOWS: Option<Limits> = Some(Limits::WINDOWS);

/// A stored item, as a test sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Text(String),
    Bytes(Vec<u8>),
}

type Hook = Box<dyn FnOnce(&Memory)>;

/// Items in memory. With the Windows limits it refuses what Windows
/// Credential Manager refuses. Any step (a get, set, delete or list) can be
/// made to fail once, or to crash, failing every step after it too, and a
/// hook can run another operation just before a chosen step.
#[derive(Default)]
pub(super) struct Memory {
    items: RefCell<BTreeMap<String, Value>>,
    windows: bool,
    steps: Cell<usize>,
    failure: Cell<Option<(usize, bool)>>,
    down: Cell<bool>,
    hook: RefCell<Option<(usize, Hook)>>,
}

impl Memory {
    pub(super) fn windows() -> Self {
        Self {
            windows: true,
            ..Self::default()
        }
    }

    pub(super) fn holding(account: &str, value: Value) -> Self {
        let store = Self::windows();
        store.insert(account, value);
        store
    }

    pub(super) fn fail_step(&self, step: usize, crash: bool) {
        self.steps.set(0);
        self.failure.set(Some((step, crash)));
    }

    pub(super) fn before_step(&self, step: usize, hook: impl FnOnce(&Memory) + 'static) {
        self.steps.set(0);
        *self.hook.borrow_mut() = Some((step, Box::new(hook)));
    }

    pub(super) fn recover(&self) {
        self.failure.set(None);
        self.down.set(false);
    }

    pub(super) fn steps(&self) -> usize {
        self.steps.get()
    }

    pub(super) fn items(&self) -> BTreeMap<String, Value> {
        self.items.borrow().clone()
    }

    pub(super) fn names(&self) -> BTreeSet<String> {
        self.items.borrow().keys().cloned().collect()
    }

    pub(super) fn insert(&self, account: &str, value: Value) {
        self.items.borrow_mut().insert(account.to_owned(), value);
    }

    pub(super) fn remove(&self, account: &str) {
        self.items.borrow_mut().remove(account);
    }

    fn step(&self) -> Result<(), KeychainError> {
        let step = self.steps.get() + 1;
        self.steps.set(step);
        let hook = self.hook.borrow_mut().take_if(|hook| hook.0 == step);
        if let Some((_, hook)) = hook {
            hook(self);
        }
        if let Some((_, crash)) = self.failure.get().filter(|failure| failure.0 == step) {
            self.down.set(crash);
            return Err(KeychainError);
        }
        match self.down.get() {
            true => Err(KeychainError),
            false => Ok(()),
        }
    }

    fn put(&self, account: &str, value: Value) -> Result<(), KeychainError> {
        self.step()?;
        let bytes = match &value {
            Value::Text(text) => text.encode_utf16().count() * 2,
            Value::Bytes(bytes) => bytes.len(),
        };
        if self.windows && (bytes > BLOB_BYTES || account.len() > ACCOUNT_BYTES) {
            return Err(KeychainError);
        }
        self.insert(account, value);
        Ok(())
    }
}

impl Store for Memory {
    fn get(&self, account: &str) -> Result<Option<Item>, KeychainError> {
        self.step()?;
        let value = self.items.borrow().get(account).cloned();
        Ok(value.map(|value| match value {
            Value::Text(text) => Item::Text(text),
            Value::Bytes(bytes) => Item::Bytes(bytes),
        }))
    }

    fn set_text(&self, account: &str, value: &str) -> Result<(), KeychainError> {
        self.put(account, Value::Text(value.to_owned()))
    }

    fn set_bytes(&self, account: &str, value: &[u8]) -> Result<(), KeychainError> {
        self.put(account, Value::Bytes(value.to_vec()))
    }

    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        self.step()?;
        self.remove(account);
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, KeychainError> {
        self.step()?;
        let prefix = prefix.to_lowercase();
        let names = self.names().into_iter();
        Ok(names
            .filter(|name| name.to_lowercase().starts_with(&prefix))
            .collect())
    }
}

/// The prefix of the account's derived names, computed as the format says.
pub(super) fn namespace_of(account: &str) -> String {
    let hash = hex::encode(Sha256::digest(account.to_lowercase().as_bytes()));
    format!("subspace-keychain#{}/", &hash[..32])
}

/// A manifest's fields, read as the format says.
pub(super) struct Fields {
    pub(super) generation: String,
    pub(super) count: usize,
    pub(super) length: usize,
    pub(super) digest: String,
}

pub(super) fn fields(value: &Value) -> Option<Fields> {
    let Value::Bytes(bytes) = value else {
        return None;
    };
    let text = std::str::from_utf8(bytes.strip_prefix(MANIFEST_PREFIX)?).ok()?;
    let parts: Vec<&str> = text.split('/').collect();
    Some(Fields {
        generation: parts[0].to_owned(),
        count: parts[1].parse().ok()?,
        length: parts[2].parse().ok()?,
        digest: parts[3].to_owned(),
    })
}

/// The account's item and every chunk its manifest names.
pub(super) fn named(account: &str, value: &Value) -> BTreeSet<String> {
    let namespace = namespace_of(account);
    let chunks = fields(value).map(|fields| {
        (0..fields.count)
            .map(|index| format!("{namespace}{}/{index}", fields.generation))
            .collect::<Vec<_>>()
    });
    std::iter::once(account.to_owned())
        .chain(chunks.unwrap_or_default())
        .collect()
}

/// Every item is the account's or a chunk its manifest names.
pub(super) fn assert_no_orphans(store: &Memory, account: &str) {
    let items = store.items();
    assert_eq!(store.names(), named(account, &items[account]));
}

/// What a build from before the split reads: text, or an error for bytes.
pub(super) fn old_build_load(
    store: &Memory,
    account: &str,
) -> Result<Option<String>, KeychainError> {
    match store.items().get(account) {
        None => Ok(None),
        Some(Value::Text(text)) => Ok(Some(text.clone())),
        Some(Value::Bytes(_)) => Err(KeychainError),
    }
}

/// Saves `secret` on a Windows store, split, and loads it back.
pub(super) fn assert_split_round_trip(secret: &str) {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, secret).unwrap();
    let items = store.items();
    assert_eq!(fields(&items[ACCOUNT]).unwrap().length, secret.len());
    assert!(items.iter().all(|(name, value)| {
        name == ACCOUNT
            || matches!(value, Value::Text(chunk)
                if chunk.encode_utf16().count() <= HEADER_UNITS + PIECE_UNITS)
    }));
    assert_eq!(
        load(&store, WINDOWS, ACCOUNT).unwrap().as_deref(),
        Some(secret)
    );
    assert_no_orphans(&store, ACCOUNT);
}
