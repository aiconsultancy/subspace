//! An in-memory store with the Windows limit, and checks on what it holds.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use crate::keychain::split::{Limits, load, save};
use crate::keychain::{KeychainError, Store};

/// One Windows credential: 2,560 bytes of UTF-16.
pub(super) const ITEM_UNITS: usize = 1_280;
/// The most a chunk may use, to stay comfortably under [`ITEM_UNITS`].
const CHUNK_UNITS: usize = 1_000;
/// The stored format's piece size and chunk limit.
pub(super) const PIECE_UNITS: usize = 960;
pub(super) const MAX_CHUNKS: usize = 256;
pub(super) const MANIFEST_PREFIX: &str = "subspace-keychain/chunks/v1/";
pub(super) const ACCOUNT: &str = "session";
pub(super) const WINDOWS: Option<Limits> = Some(Limits::WINDOWS);

/// Items in memory. Like Windows Credential Manager it can refuse items
/// over [`ITEM_UNITS`], and it can fail one write, staying down after it as
/// if the process had stopped there.
#[derive(Default)]
pub(super) struct Memory {
    items: RefCell<BTreeMap<String, String>>,
    limit: Option<usize>,
    writes: Cell<usize>,
    /// The write that fails, counting from 1, and whether the store stays down.
    failure: Cell<Option<(usize, bool)>>,
    down: Cell<bool>,
}

impl Memory {
    pub(super) fn windows() -> Self {
        Self {
            limit: Some(ITEM_UNITS),
            ..Self::default()
        }
    }

    pub(super) fn holding(account: &str, value: &str) -> Self {
        let store = Self::windows();
        store.insert(account, value);
        store
    }

    pub(super) fn fail_write(&self, write: usize, crash: bool) {
        self.writes.set(0);
        self.failure.set(Some((write, crash)));
    }

    pub(super) fn recover(&self) {
        self.failure.set(None);
        self.down.set(false);
    }

    pub(super) fn items(&self) -> BTreeMap<String, String> {
        self.items.borrow().clone()
    }

    pub(super) fn names(&self) -> BTreeSet<String> {
        self.items.borrow().keys().cloned().collect()
    }

    pub(super) fn insert(&self, account: &str, value: &str) {
        self.items
            .borrow_mut()
            .insert(account.to_owned(), value.to_owned());
    }

    pub(super) fn remove(&self, account: &str) {
        self.items.borrow_mut().remove(account);
    }
}

impl Store for Memory {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError> {
        match self.down.get() {
            true => Err(KeychainError),
            false => Ok(self.items.borrow().get(account).cloned()),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), KeychainError> {
        let write = self.writes.get() + 1;
        self.writes.set(write);
        if let Some((_, crash)) = self.failure.get().filter(|(at, _)| *at == write) {
            self.down.set(crash);
            return Err(KeychainError);
        }
        let too_long = self
            .limit
            .is_some_and(|limit| value.encode_utf16().count() > limit);
        if self.down.get() || too_long {
            return Err(KeychainError);
        }
        self.insert(account, value);
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        if self.down.get() {
            return Err(KeychainError);
        }
        self.remove(account);
        Ok(())
    }
}

/// The account and every chunk its item names, read as the format says.
pub(super) fn named(account: &str, primary: &str) -> BTreeSet<String> {
    let chunks = primary.strip_prefix(MANIFEST_PREFIX).map(|fields| {
        let fields: Vec<&str> = fields.split('/').collect();
        let count: usize = fields[1].parse().unwrap();
        (0..count)
            .map(|index| format!("{account}#chunk/{}/{index}", fields[0]))
            .collect::<Vec<_>>()
    });
    std::iter::once(account.to_owned())
        .chain(chunks.unwrap_or_default())
        .collect()
}

/// Every item in `store` is the account's or a chunk its manifest names.
pub(super) fn assert_no_orphans(store: &Memory) {
    let primary = store.items()[ACCOUNT].clone();
    assert_eq!(store.names(), named(ACCOUNT, &primary));
}

/// Saves `secret` on a Windows store, split, and loads it back.
pub(super) fn assert_split_round_trip(secret: &str) {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, secret).unwrap();
    let items = store.items();
    assert!(items[ACCOUNT].starts_with(MANIFEST_PREFIX));
    assert!(
        items.iter().all(|(name, value)| {
            name == ACCOUNT || value.encode_utf16().count() <= CHUNK_UNITS
        })
    );
    assert_eq!(
        load(&store, WINDOWS, ACCOUNT).unwrap().as_deref(),
        Some(secret)
    );
    assert_no_orphans(&store);
}
