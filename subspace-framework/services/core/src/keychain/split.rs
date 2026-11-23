//! Secrets longer than one stored item can hold, split across several items.
//!
//! The account's own item holds the secret itself, exactly as earlier builds
//! wrote it, whenever it fits. A longer secret is split into chunks and the
//! account's item holds a short manifest instead:
//!
//! - A value without the [`TAG`] prefix is the secret itself.
//! - `TAG` + `plain/` + the secret is a secret that itself starts with `TAG`.
//! - `TAG` + `chunks/v1/<generation>/<count>/<bytes>` is a manifest: the
//!   secret is `count` chunks, `bytes` long in UTF-8. The generation is 32
//!   random hexadecimal digits, new for every save.
//! - Any other value with the prefix is a format this build cannot read.
//!
//! Chunk `index` is stored under `<account>#chunk/<generation>/<index>` as
//! `<generation>/<index>/` and its piece of the secret. A save writes the new
//! chunks, then the manifest, and only then deletes the previous chunks, so a
//! save that stops part way leaves the previous secret readable. A load that
//! finds a chunk missing, or one written by another save, fails rather than
//! return part of a secret.

use std::sync::Mutex;

use super::{KeychainError, Store};
use crate::vault::lock;

/// Marks a value that is not the secret itself.
const TAG: &str = "subspace-keychain/";
/// After [`TAG`]: a secret that itself starts with the tag.
const ESCAPED: &str = "plain/";
/// After [`TAG`]: a version 1 manifest.
const MANIFEST: &str = "chunks/v1/";
const GENERATION_DIGITS: usize = 32;
/// The most chunks one secret may use: 240 KiB of ASCII in Windows chunks.
const MAX_CHUNKS: usize = 256;
/// Windows Credential Manager holds at most 2,560 bytes in one credential
/// (`CRED_MAX_CREDENTIAL_BLOB_SIZE`) and the keyring crate stores a password
/// there as UTF-16, so 1,280 code units. Earlier builds had the same limit.
const WINDOWS_ITEM_UNITS: usize = 1_280;
/// A chunk's piece of the secret: with its header of at most 37 units, a
/// chunk stays more than a fifth under [`WINDOWS_ITEM_UNITS`].
const WINDOWS_PIECE_UNITS: usize = 960;

/// Serialises split loads, saves and deletes in this process, so a load
/// never reads a manifest whose chunks a save is deleting.
static SPLIT: Mutex<()> = Mutex::new(());

/// How much one stored item holds, in UTF-16 code units.
#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    /// The longest value kept as one item, which every earlier build reads.
    single: usize,
    /// The longest piece of a split secret.
    piece: usize,
}

impl Limits {
    pub(super) const WINDOWS: Self = Self {
        single: WINDOWS_ITEM_UNITS,
        piece: WINDOWS_PIECE_UNITS,
    };
}

/// What an account's own item holds.
enum Stored<'a> {
    Plain(&'a str),
    /// The fields of a manifest, after its prefix.
    Manifest(&'a str),
    Unknown,
}

struct Manifest {
    generation: String,
    count: usize,
    bytes: usize,
}

/// The items one save writes: its chunks by name, then the account's item.
struct Plan {
    chunks: Vec<(String, String)>,
    primary: String,
}

/// The secret under `account`. Without limits, one item holds any secret.
pub(super) fn load(
    store: &impl Store,
    limits: Option<Limits>,
    account: &str,
) -> Result<Option<String>, KeychainError> {
    match limits {
        None => store.get(account),
        Some(_) => load_split(store, account),
    }
}

pub(super) fn save(
    store: &impl Store,
    limits: Option<Limits>,
    account: &str,
    secret: &str,
) -> Result<(), KeychainError> {
    match limits {
        None => store.set(account, secret),
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
        Some(_) => delete_split(store, account),
    }
}

fn load_split(store: &impl Store, account: &str) -> Result<Option<String>, KeychainError> {
    let _split = lock(&SPLIT);
    let value = store.get(account)?;
    value
        .map(|value| decode(store, account, &value))
        .transpose()
}

/// Reading the previous item is best effort: one that cannot be read is
/// replaced, as earlier builds did, at the cost of its chunks.
fn save_split(
    store: &impl Store,
    limits: Limits,
    account: &str,
    secret: &str,
) -> Result<(), KeychainError> {
    let _split = lock(&SPLIT);
    let previous = chunk_names(account, store.get(account).ok().flatten().as_deref());
    let plan = plan(account, secret, limits)?;
    write(store, account, &plan)
        .inspect_err(|_| forget(store, plan.chunks.iter().map(|(name, _)| name)))?;
    forget(store, &previous);
    Ok(())
}

/// Deletes the chunks before the manifest that names them, so a delete that
/// stops part way can be repeated. An unreadable item is deleted alone.
fn delete_split(store: &impl Store, account: &str) -> Result<(), KeychainError> {
    let _split = lock(&SPLIT);
    let chunks = chunk_names(account, store.get(account).ok().flatten().as_deref());
    chunks.iter().try_for_each(|name| store.delete(name))?;
    store.delete(account)
}

fn decode(store: &impl Store, account: &str, value: &str) -> Result<String, KeychainError> {
    match classify(value) {
        Stored::Plain(secret) => Ok(secret.to_owned()),
        Stored::Manifest(fields) => join(store, account, read_manifest(fields)),
        Stored::Unknown => Err(KeychainError),
    }
}

fn join(
    store: &impl Store,
    account: &str,
    manifest: Option<Manifest>,
) -> Result<String, KeychainError> {
    let manifest = manifest.ok_or(KeychainError)?;
    let pieces = (0..manifest.count)
        .map(|index| read_chunk(store, account, &manifest, index))
        .collect::<Result<Vec<_>, _>>()?;
    assemble(&manifest, &pieces)
}

fn read_chunk(
    store: &impl Store,
    account: &str,
    manifest: &Manifest,
    index: usize,
) -> Result<String, KeychainError> {
    let value = store.get(&chunk_name(account, &manifest.generation, index))?;
    strip_header(value, &chunk_header(&manifest.generation, index))
}

fn plan(account: &str, secret: &str, limits: Limits) -> Result<Plan, KeychainError> {
    single(secret, limits).map_or_else(
        || split(account, secret, limits, &generation()),
        |primary| {
            Ok(Plan {
                chunks: Vec::new(),
                primary,
            })
        },
    )
}

fn split(
    account: &str,
    secret: &str,
    limits: Limits,
    generation: &str,
) -> Result<Plan, KeychainError> {
    let pieces = pieces(secret, limits.piece)?;
    let chunks = pieces.iter().enumerate().map(|(index, piece)| {
        let value = chunk_header(generation, index) + piece;
        (chunk_name(account, generation, index), value)
    });
    Ok(Plan {
        chunks: chunks.collect(),
        primary: manifest(generation, pieces.len(), secret.len()),
    })
}

fn write(store: &impl Store, account: &str, plan: &Plan) -> Result<(), KeychainError> {
    plan.chunks
        .iter()
        .try_for_each(|(name, value)| store.set(name, value))?;
    store.set(account, &plan.primary)
}

/// Deletes the named items, ignoring failures: they are no longer read.
fn forget<'a>(store: &impl Store, names: impl IntoIterator<Item = &'a String>) {
    names.into_iter().for_each(|name| {
        let _ = store.delete(name);
    });
}

/// The chunks the account's item `value` names, if it is a manifest.
fn chunk_names(account: &str, value: Option<&str>) -> Vec<String> {
    let manifest = value
        .map(classify)
        .and_then(Stored::manifest)
        .and_then(read_manifest);
    manifest.map_or_else(Vec::new, |manifest| {
        (0..manifest.count)
            .map(|index| chunk_name(account, &manifest.generation, index))
            .collect()
    })
}

impl<'a> Stored<'a> {
    fn manifest(self) -> Option<&'a str> {
        match self {
            Stored::Manifest(fields) => Some(fields),
            Stored::Plain(_) | Stored::Unknown => None,
        }
    }
}

fn classify(value: &str) -> Stored<'_> {
    let Some(tagged) = value.strip_prefix(TAG) else {
        return Stored::Plain(value);
    };
    match (tagged.strip_prefix(ESCAPED), tagged.strip_prefix(MANIFEST)) {
        (Some(secret), _) => Stored::Plain(secret),
        (None, Some(fields)) => Stored::Manifest(fields),
        (None, None) => Stored::Unknown,
    }
}

fn read_manifest(fields: &str) -> Option<Manifest> {
    let [generation, count, bytes] = fields.split('/').collect::<Vec<_>>()[..] else {
        return None;
    };
    let valid = generation.len() == GENERATION_DIGITS
        && generation
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    let count = count
        .parse()
        .ok()
        .filter(|count| valid && (1..=MAX_CHUNKS).contains(count))?;
    Some(Manifest {
        generation: generation.to_owned(),
        count,
        bytes: bytes.parse().ok()?,
    })
}

/// The one item that holds `secret`, if it fits in one.
fn single(secret: &str, limits: Limits) -> Option<String> {
    let value = match secret.starts_with(TAG) {
        true => format!("{TAG}{ESCAPED}{secret}"),
        false => secret.to_owned(),
    };
    (value.encode_utf16().count() <= limits.single).then_some(value)
}

/// `secret` in pieces of at most `units` UTF-16 code units, split between
/// characters, or an error when it needs more than [`MAX_CHUNKS`].
fn pieces(secret: &str, units: usize) -> Result<Vec<&str>, KeychainError> {
    let mut pieces = Vec::new();
    let (mut start, mut length) = (0, 0);
    for (at, character) in secret.char_indices() {
        if length + character.len_utf16() > units {
            pieces.push(&secret[start..at]);
            (start, length) = (at, 0);
        }
        length += character.len_utf16();
    }
    pieces.push(&secret[start..]);
    match pieces.len() <= MAX_CHUNKS {
        true => Ok(pieces),
        false => Err(KeychainError),
    }
}

/// The piece in a chunk's `value`, unless it is missing or another save's.
fn strip_header(value: Option<String>, header: &str) -> Result<String, KeychainError> {
    value
        .as_deref()
        .and_then(|value| value.strip_prefix(header))
        .map(str::to_owned)
        .ok_or(KeychainError)
}

/// The secret the pieces make, unless it is not the manifest's length.
fn assemble(manifest: &Manifest, pieces: &[String]) -> Result<String, KeychainError> {
    let secret = pieces.concat();
    match secret.len() == manifest.bytes {
        true => Ok(secret),
        false => Err(KeychainError),
    }
}

fn generation() -> String {
    format!(
        "{:0width$x}",
        rand::random::<u128>(),
        width = GENERATION_DIGITS
    )
}

fn manifest(generation: &str, count: usize, bytes: usize) -> String {
    format!("{TAG}{MANIFEST}{generation}/{count}/{bytes}")
}

fn chunk_name(account: &str, generation: &str, index: usize) -> String {
    format!("{account}#chunk/{generation}/{index}")
}

fn chunk_header(generation: &str, index: usize) -> String {
    format!("{generation}/{index}/")
}
