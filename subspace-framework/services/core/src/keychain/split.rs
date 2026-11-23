//! Secrets longer than one stored item can hold, split across several items.
//!
//! The account's own item is the one commit point, for this build and every
//! earlier one. Text there is always the secret itself, verbatim, as every
//! earlier build wrote it; a longer secret is chunks under derived names and,
//! in the account's item, a manifest in bytes that no text write can produce
//! (the `format` module has the layout).
//!
//! A save reads the account's item, writes the new chunks, writes the
//! account's item, the commit, and then deletes the chunks of the one
//! generation it read. It never searches the store, so of two saves at once
//! the one that commits last stays readable and the other's chunks are left
//! over. A delete deletes the account's item, the commit, and then sweeps,
//! best effort, every chunk of the account under this service. A load fails,
//! rather than return anything else, unless the chunks are exactly the
//! pieces a save makes of a secret with the manifest's length and digest.

mod format;
mod read;

use zeroize::Zeroizing;

use super::{Item, KeychainError, Store};
use format::{
    chunk_header, chunk_name, chunk_of, chunk_pattern, digest, fits, generation, manifest,
    namespace, pieces, read_manifest, room,
};
use read::decode;

/// Windows Credential Manager holds at most 2,560 bytes in one credential
/// (`CRED_MAX_CREDENTIAL_BLOB_SIZE`) and the keyring crate stores a password
/// there as UTF-16, so 1,280 code units. Earlier builds had the same limit.
const WINDOWS_ITEM_UNITS: usize = 1_280;
/// A chunk's piece of the secret: with its header of at most 37 units, a
/// chunk stays more than a fifth under [`WINDOWS_ITEM_UNITS`].
const WINDOWS_PIECE_UNITS: usize = 960;
/// `CRED_MAX_USERNAME_LENGTH`, which keyring applies to the account in bytes.
const WINDOWS_ACCOUNT_BYTES: usize = 513;
/// `CRED_MAX_GENERIC_TARGET_NAME_LENGTH`, which keyring applies to
/// `<account>.<service>` in bytes.
const WINDOWS_TARGET_BYTES: usize = 32_767;

/// What one stored item holds, in UTF-16 code units, and the longest names.
#[derive(Clone, Copy)]
pub(super) struct Limits {
    /// The longest secret kept as one item, which every earlier build reads.
    item: usize,
    /// The longest piece of a split secret.
    piece: usize,
    /// The longest account name, in bytes.
    account: usize,
    /// The longest target name, `<account>.<service>`, in bytes.
    target: usize,
}

impl Limits {
    pub(super) const WINDOWS: Self = Self {
        item: WINDOWS_ITEM_UNITS,
        piece: WINDOWS_PIECE_UNITS,
        account: WINDOWS_ACCOUNT_BYTES,
        target: WINDOWS_TARGET_BYTES,
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
    let namespace = namespace(store.service(), account, limits.account)?;
    let item = store.get(account)?;
    item.map(|item| decode(store, &namespace, limits, item))
        .transpose()
}

/// Reading the previous item is best effort: one that cannot be read is
/// replaced, as earlier builds did, and its chunks are left over.
fn save_split(
    store: &impl Store,
    limits: Limits,
    account: &str,
    secret: &str,
) -> Result<(), KeychainError> {
    let namespace = namespace(store.service(), account, limits.account)?;
    let previous = chunk_names(&namespace, store.get(account).ok().flatten());
    let plan = plan(store.service(), &namespace, secret, limits)?;
    write(store, account, secret, &plan)
        .inspect_err(|_| abandon(store, account, &namespace, &plan))?;
    forget(store, &previous);
    Ok(())
}

/// Once the account's item is gone the secret is gone, so nothing after
/// that commit can fail the delete.
fn delete_split(store: &impl Store, limits: Limits, account: &str) -> Result<(), KeychainError> {
    let namespace = namespace(store.service(), account, limits.account)?;
    store.delete(account)?;
    sweep(store, account, &namespace);
    Ok(())
}

fn plan(
    service: &str,
    namespace: &str,
    secret: &str,
    limits: Limits,
) -> Result<Plan, KeychainError> {
    match fits(secret, limits.item) {
        true => Ok(Plan {
            chunks: Vec::new(),
            manifest: None,
        }),
        false => split(service, namespace, secret, limits, &generation()),
    }
}

fn split(
    service: &str,
    namespace: &str,
    secret: &str,
    limits: Limits,
    generation: &str,
) -> Result<Plan, KeychainError> {
    room(service, limits.target)?;
    let pieces = pieces(secret, limits.piece)?;
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

/// Deletes a failed save's chunks, unless the account's item names them: a
/// commit that reported a failure may still have landed.
fn abandon(store: &impl Store, account: &str, namespace: &str, plan: &Plan) {
    let live = chunk_names(namespace, store.get(account).ok().flatten());
    let written: Vec<String> = plan.chunks.iter().map(|(name, _)| name.clone()).collect();
    forget(store, &unnamed(&written, &live));
}

/// Deletes, best effort, every chunk of the account under this service that
/// its item does not name now.
fn sweep(store: &impl Store, account: &str, namespace: &str) {
    let live = chunk_names(namespace, store.get(account).ok().flatten());
    let pattern = chunk_pattern(namespace, store.service());
    let found = store.find(&pattern).unwrap_or_default();
    forget(store, &obsolete(&found, namespace, store.service(), &live));
}

/// The account's chunks under `service` among the `found` target names,
/// apart from the `live` ones.
fn obsolete(found: &[String], namespace: &str, service: &str, live: &[String]) -> Vec<String> {
    let chunks: Vec<String> = found
        .iter()
        .filter_map(|target| chunk_of(target, namespace, service))
        .collect();
    unnamed(&chunks, live)
}

fn unnamed(names: &[String], live: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| !live.contains(name))
        .cloned()
        .collect()
}

/// Deletes the named items, ignoring failures: nothing reads them.
fn forget(store: &impl Store, names: &[String]) {
    names.iter().for_each(|name| {
        let _ = store.delete(name);
    });
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

fn manifest_bytes(item: Item) -> Option<Zeroizing<Vec<u8>>> {
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
