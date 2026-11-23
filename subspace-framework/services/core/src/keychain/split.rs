//! Secrets longer than one stored item can hold, split across several items.
//!
//! The account's own item is the one commit point, for this build and every
//! earlier one. Text there is always the secret itself, verbatim, as every
//! earlier build wrote it; a longer secret is chunks under derived names and,
//! in the account's item, a manifest in bytes that no text write can produce
//! (the `format` module has the layout).
//!
//! A save writes the new chunks, then the account's item, then sweeps every
//! chunk of the account that the item does not name; a save that fails
//! before the account's item is written deletes its own chunks. A delete
//! deletes the account's item, then sweeps. A load fails, rather than return
//! anything else, unless the chunks make a secret with the manifest's length
//! and digest.

mod format;

use zeroize::Zeroizing;

use super::{Item, KeychainError, Store};
use format::{
    Manifest, chunk_header, chunk_name, concat, digest, fits, generation, manifest, namespace,
    piece, pieces, read_manifest, verify,
};

/// Windows Credential Manager holds at most 2,560 bytes in one credential
/// (`CRED_MAX_CREDENTIAL_BLOB_SIZE`) and the keyring crate stores a password
/// there as UTF-16, so 1,280 code units. Earlier builds had the same limit.
const WINDOWS_ITEM_UNITS: usize = 1_280;
/// A chunk's piece of the secret: with its header of at most 37 units, a
/// chunk stays more than a fifth under [`WINDOWS_ITEM_UNITS`].
const WINDOWS_PIECE_UNITS: usize = 960;
/// `CRED_MAX_USERNAME_LENGTH`, which keyring applies to the account in bytes.
const WINDOWS_ACCOUNT_BYTES: usize = 513;

/// What one stored item holds, in UTF-16 code units, and the longest account.
#[derive(Clone, Copy)]
pub(super) struct Limits {
    /// The longest secret kept as one item, which every earlier build reads.
    item: usize,
    /// The longest piece of a split secret.
    piece: usize,
    /// The longest account name, in bytes.
    account: usize,
}

impl Limits {
    pub(super) const WINDOWS: Self = Self {
        item: WINDOWS_ITEM_UNITS,
        piece: WINDOWS_PIECE_UNITS,
        account: WINDOWS_ACCOUNT_BYTES,
    };
}

/// What a save writes: its chunks by name, then the account's item, which is
/// the secret itself unless there is a manifest.
struct Plan {
    chunks: Vec<(String, Zeroizing<String>)>,
    manifest: Option<Vec<u8>>,
}

/// The secret under `account`. Without limits, one item holds any secret.
pub(super) fn load(
    store: &impl Store,
    limits: Option<Limits>,
    account: &str,
) -> Result<Option<String>, KeychainError> {
    match limits {
        None => load_one(store, account),
        Some(limits) => load_split(store, limits, account),
    }
}

pub(super) fn save(
    store: &impl Store,
    limits: Option<Limits>,
    account: &str,
    secret: &str,
) -> Result<(), KeychainError> {
    match limits {
        None => store.set_text(account, secret),
        Some(limits) => save_split(store, limits, account, secret),
    }
}

/// Deletes the secret and its chunks; missing ones are not an error.
pub(super) fn delete(
    store: &impl Store,
    limits: Option<Limits>,
    account: &str,
) -> Result<(), KeychainError> {
    match limits {
        None => store.delete(account),
        Some(limits) => delete_split(store, limits, account),
    }
}

fn load_one(store: &impl Store, account: &str) -> Result<Option<String>, KeychainError> {
    store.get(account)?.map(text).transpose()
}

fn load_split(
    store: &impl Store,
    limits: Limits,
    account: &str,
) -> Result<Option<String>, KeychainError> {
    let namespace = namespace(account, limits.account)?;
    let item = store.get(account)?;
    item.map(|item| decode(store, &namespace, item)).transpose()
}

/// The sweep after the account's item is written is best effort: the secret
/// is saved, and the next save or delete sweeps again.
fn save_split(
    store: &impl Store,
    limits: Limits,
    account: &str,
    secret: &str,
) -> Result<(), KeychainError> {
    let namespace = namespace(account, limits.account)?;
    let plan = plan(&namespace, secret, limits)?;
    write(store, account, secret, &plan).inspect_err(|_| forget(store, &plan))?;
    let _ = sweep(store, account, &namespace);
    Ok(())
}

/// Deletes the account's item, the commit, before the chunks, so a delete
/// that stops part way leaves no secret.
fn delete_split(store: &impl Store, limits: Limits, account: &str) -> Result<(), KeychainError> {
    let namespace = namespace(account, limits.account)?;
    store.delete(account)?;
    sweep(store, account, &namespace)
}

fn decode(store: &impl Store, namespace: &str, item: Item) -> Result<String, KeychainError> {
    match item {
        Item::Text(secret) => Ok(secret),
        Item::Bytes(bytes) => join(store, namespace, read_manifest(&bytes)),
    }
}

fn join(
    store: &impl Store,
    namespace: &str,
    manifest: Option<Manifest>,
) -> Result<String, KeychainError> {
    let manifest = manifest.ok_or(KeychainError)?;
    let pieces = (0..manifest.count)
        .map(|index| read_chunk(store, namespace, &manifest, index))
        .collect::<Result<Vec<_>, _>>()?;
    let secret = concat(&pieces);
    let digest = digest(&manifest.generation, &secret);
    verify(&manifest, secret, &digest)
}

fn read_chunk(
    store: &impl Store,
    namespace: &str,
    manifest: &Manifest,
    index: usize,
) -> Result<Zeroizing<String>, KeychainError> {
    let item = store.get(&chunk_name(namespace, &manifest.generation, index))?;
    piece(item, &chunk_header(&manifest.generation, index))
}

fn plan(namespace: &str, secret: &str, limits: Limits) -> Result<Plan, KeychainError> {
    match fits(secret, limits.item) {
        true => Ok(Plan {
            chunks: Vec::new(),
            manifest: None,
        }),
        false => split(namespace, secret, limits.piece, &generation()),
    }
}

fn split(
    namespace: &str,
    secret: &str,
    units: usize,
    generation: &str,
) -> Result<Plan, KeychainError> {
    let pieces = pieces(secret, units)?;
    let chunks = pieces.iter().enumerate().map(|(index, piece)| {
        let value = Zeroizing::new(chunk_header(generation, index) + piece);
        (chunk_name(namespace, generation, index), value)
    });
    let digest = digest(generation, secret);
    Ok(Plan {
        chunks: chunks.collect(),
        manifest: Some(manifest(generation, pieces.len(), secret.len(), &digest)),
    })
}

fn write(
    store: &impl Store,
    account: &str,
    secret: &str,
    plan: &Plan,
) -> Result<(), KeychainError> {
    plan.chunks
        .iter()
        .try_for_each(|(name, value)| store.set_text(name, value))?;
    match &plan.manifest {
        None => store.set_text(account, secret),
        Some(manifest) => store.set_bytes(account, manifest),
    }
}

/// Deletes a failed save's chunks, ignoring failures: nothing names them.
fn forget(store: &impl Store, plan: &Plan) {
    plan.chunks.iter().for_each(|(name, _)| {
        let _ = store.delete(name);
    });
}

/// Deletes every chunk of the account that its item does not name now.
fn sweep(store: &impl Store, account: &str, namespace: &str) -> Result<(), KeychainError> {
    let live = chunk_names(namespace, store.get(account)?);
    let found = store.list(namespace)?;
    found
        .iter()
        .filter(|name| !live.iter().any(|live| live.eq_ignore_ascii_case(name)))
        .try_for_each(|name| store.delete(name))
}

/// The chunks the account's item names, if it is a manifest.
fn chunk_names(namespace: &str, item: Option<Item>) -> Vec<String> {
    let manifest = item
        .and_then(manifest_bytes)
        .and_then(|bytes| read_manifest(&bytes));
    manifest.map_or_else(Vec::new, |manifest| {
        (0..manifest.count)
            .map(|index| chunk_name(namespace, &manifest.generation, index))
            .collect()
    })
}

fn manifest_bytes(item: Item) -> Option<Vec<u8>> {
    match item {
        Item::Bytes(bytes) => Some(bytes),
        Item::Text(secret) => {
            let _wiped = Zeroizing::new(secret);
            None
        }
    }
}

/// An item that, without limits, must be text.
fn text(item: Item) -> Result<String, KeychainError> {
    match item {
        Item::Text(text) => Ok(text),
        Item::Bytes(_) => Err(KeychainError),
    }
}
