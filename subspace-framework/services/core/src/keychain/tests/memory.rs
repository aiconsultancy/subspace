//! An in-memory credential store that answers as Windows Credential Manager
//! does through keyring, and checks on what it holds.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use regex::Regex;
use zeroize::Zeroizing;

use crate::keychain::split::Limits;
use crate::keychain::{Item, KeychainError, Store};

/// `CRED_MAX_CREDENTIAL_BLOB_SIZE`: the bytes one Windows credential holds.
pub(super) const BLOB_BYTES: usize = 2_560;
/// The same limit for a password, which keyring stores as UTF-16.
pub(super) const ITEM_UNITS: usize = BLOB_BYTES / 2;
/// `CRED_MAX_USERNAME_LENGTH` and `CRED_MAX_GENERIC_TARGET_NAME_LENGTH`,
/// which keyring checks in bytes.
pub(super) const ACCOUNT_BYTES: usize = 513;
pub(super) const TARGET_BYTES: usize = 32_767;
/// The stored format's piece size, chunk header bound and chunk limit.
pub(super) const PIECE_UNITS: usize = 960;
pub(super) const HEADER_UNITS: usize = 37;
pub(super) const MAX_CHUNKS: usize = 256;
pub(super) const MANIFEST_PREFIX: &[u8] = b"\xff\xdfsubspace-keychain/chunks/v2/";
pub(super) const SERVICE: &str = "app.example";
pub(super) const ACCOUNT: &str = "session";
pub(super) const WINDOWS: Option<Limits> = Some(Limits::WINDOWS);

/// A stored item, as keyring reads it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Text(String),
    Bytes(Vec<u8>),
}

/// How a chosen step fails.
#[derive(Clone, Copy, Debug)]
pub(super) enum Failure {
    /// It fails and changes nothing.
    Error,
    /// It fails, and so does every later step, as if the process stopped.
    Crash,
    /// It changes the store but reports a failure.
    Lands,
}

type Hook = Box<dyn FnOnce(&Memory)>;

/// Every credential, by target name, whatever its service.
#[derive(Default)]
struct Credentials {
    blobs: RefCell<BTreeMap<String, Vec<u8>>>,
    windows: bool,
    steps: Cell<usize>,
    failure: Cell<Option<(usize, Failure)>>,
    down: Cell<bool>,
    hook: RefCell<Option<(usize, Hook)>>,
    strays: RefCell<Vec<String>>,
}

/// The credentials under one service. With the Windows limits it refuses
/// what keyring refuses on Windows. Any step (a get, set, delete or find) can
/// be made to fail, a hook can run another operation just before a chosen
/// step, and a search can return stray names.
pub(super) struct Memory {
    service: String,
    credentials: Rc<Credentials>,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            service: SERVICE.to_owned(),
            credentials: Rc::default(),
        }
    }
}

impl Memory {
    pub(super) fn windows() -> Self {
        let credentials = Credentials {
            windows: true,
            ..Credentials::default()
        };
        Self {
            credentials: Rc::new(credentials),
            ..Self::default()
        }
    }

    pub(super) fn holding(account: &str, value: Value) -> Self {
        let store = Self::windows();
        store.insert(account, value);
        store
    }

    /// The same credentials, under another service.
    pub(super) fn under(&self, service: &str) -> Self {
        Self {
            service: service.to_owned(),
            credentials: Rc::clone(&self.credentials),
        }
    }

    pub(super) fn fail_step(&self, step: usize, failure: Failure) {
        self.credentials.steps.set(0);
        self.credentials.failure.set(Some((step, failure)));
    }

    pub(super) fn before_step(&self, step: usize, hook: impl FnOnce(&Memory) + 'static) {
        self.credentials.steps.set(0);
        *self.credentials.hook.borrow_mut() = Some((step, Box::new(hook)));
    }

    pub(super) fn recover(&self) {
        self.credentials.failure.set(None);
        self.credentials.down.set(false);
    }

    pub(super) fn steps(&self) -> usize {
        self.credentials.steps.get()
    }

    /// Makes every later search also return `target`.
    pub(super) fn stray(&self, target: &str) {
        self.credentials.strays.borrow_mut().push(target.to_owned());
    }

    /// The items under this service, by account, read back as keyring does.
    pub(super) fn items(&self) -> BTreeMap<String, Value> {
        let suffix = format!(".{}", self.service);
        let blobs = self.credentials.blobs.borrow();
        blobs
            .iter()
            .filter_map(|(target, blob)| {
                let account = target.strip_suffix(&suffix)?;
                Some((account.to_owned(), classify(blob)))
            })
            .collect()
    }

    pub(super) fn names(&self) -> BTreeSet<String> {
        self.items().into_keys().collect()
    }

    /// Every target name, under any service.
    pub(super) fn targets(&self) -> BTreeSet<String> {
        self.credentials.blobs.borrow().keys().cloned().collect()
    }

    pub(super) fn insert(&self, account: &str, value: Value) {
        self.insert_target(&self.target(account), value);
    }

    pub(super) fn insert_target(&self, target: &str, value: Value) {
        let blob = match value {
            Value::Text(text) => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            Value::Bytes(bytes) => bytes,
        };
        self.credentials
            .blobs
            .borrow_mut()
            .insert(target.to_owned(), blob);
    }

    pub(super) fn remove(&self, account: &str) {
        self.credentials
            .blobs
            .borrow_mut()
            .remove(&self.target(account));
    }

    fn target(&self, account: &str) -> String {
        format!("{account}.{}", self.service)
    }

    /// Counts a step: an error if it fails before changing anything, or
    /// `true` if it changes the store but reports a failure.
    fn step(&self) -> Result<bool, KeychainError> {
        let credentials = &self.credentials;
        let step = credentials.steps.get() + 1;
        credentials.steps.set(step);
        let hook = credentials.hook.borrow_mut().take_if(|hook| hook.0 == step);
        if let Some((_, hook)) = hook {
            hook(self);
        }
        if credentials.down.get() {
            return Err(KeychainError);
        }
        let failure = credentials
            .failure
            .get()
            .filter(|failure| failure.0 == step);
        match failure.map(|failure| failure.1) {
            None => Ok(false),
            Some(Failure::Lands) => Ok(true),
            Some(Failure::Error) => Err(KeychainError),
            Some(Failure::Crash) => {
                credentials.down.set(true);
                Err(KeychainError)
            }
        }
    }

    fn put(&self, account: &str, value: Value) -> Result<(), KeychainError> {
        let lands = self.step()?;
        let target = self.target(account);
        let bytes = match &value {
            Value::Text(text) => text.encode_utf16().count() * 2,
            Value::Bytes(bytes) => bytes.len(),
        };
        let too_long =
            bytes > BLOB_BYTES || account.len() > ACCOUNT_BYTES || target.len() > TARGET_BYTES;
        if self.credentials.windows && too_long {
            return Err(KeychainError);
        }
        self.insert_target(&target, value);
        report(lands)
    }
}

fn report(lands: bool) -> Result<(), KeychainError> {
    match lands {
        true => Err(KeychainError),
        false => Ok(()),
    }
}

/// What keyring's Windows store reads back (`extract_password`): text when
/// the blob is UTF-16LE, its bytes, as `BadEncoding`, otherwise.
pub(super) fn classify(blob: &[u8]) -> Value {
    let (pairs, odd) = blob.as_chunks::<2>();
    if !odd.is_empty() {
        return Value::Bytes(blob.to_vec());
    }
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16(&units).map_or_else(|_| Value::Bytes(blob.to_vec()), Value::Text)
}

impl Store for Memory {
    fn service(&self) -> &str {
        &self.service
    }

    fn get(&self, account: &str) -> Result<Option<Item>, KeychainError> {
        self.step()?;
        let blob = self
            .credentials
            .blobs
            .borrow()
            .get(&self.target(account))
            .cloned();
        Ok(blob.map(|blob| match classify(&blob) {
            Value::Text(text) => Item::Text(text),
            Value::Bytes(bytes) => Item::Bytes(Zeroizing::new(bytes)),
        }))
    }

    fn set_text(&self, account: &str, value: &str) -> Result<(), KeychainError> {
        self.put(account, Value::Text(value.to_owned()))
    }

    fn set_bytes(&self, account: &str, value: &[u8]) -> Result<(), KeychainError> {
        self.put(account, Value::Bytes(value.to_vec()))
    }

    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        let lands = self.step()?;
        self.remove(account);
        report(lands)
    }

    fn find(&self, pattern: &str) -> Result<Vec<String>, KeychainError> {
        self.step()?;
        let pattern = Regex::new(pattern).map_err(|_| KeychainError)?;
        let mut found: Vec<String> = self
            .targets()
            .into_iter()
            .filter(|target| pattern.is_match(target))
            .collect();
        found.extend(self.credentials.strays.borrow().iter().cloned());
        Ok(found)
    }
}
