//! The stored format of a split secret.
//!
//! The manifest, in the account's own item, is the bytes `0xFF 0xDF`, then
//! `subspace-keychain/chunks/v2/<generation>/<count>/<length>/<digest>`, 141
//! bytes in all. `0xFF` never occurs in UTF-8, and read as UTF-16LE the first
//! code unit is `0xDFFF`, a low surrogate with nothing before it, so the
//! bytes are never text in either encoding, whatever their length; the odd
//! length alone makes Windows hand them back as bytes. The generation is 32
//! random hexadecimal digits, new for every save; `count` (3 digits) and
//! `length` (9 digits, the secret's UTF-8 length) are zero-padded; the
//! digest is the SHA-256 of the generation followed by the secret, in
//! hexadecimal.
//!
//! Chunk `index` is the text `<generation>/<index>/` and its piece of the
//! secret, under the derived account name
//! `subspace-keychain#<scope hash>/<generation>/<index>`. The scope hash is
//! the first 32 hexadecimal digits of the SHA-256 of
//! `<service length>:<service><account>`, both with ASCII letters in lower
//! case, as Windows compares names without case; other letters keep their
//! case, so accounts that differ only in the case of a non-ASCII letter do
//! not share chunks. Derived names are at most 87 bytes, whatever the
//! account. An account containing a NUL or the marker, in any case, is
//! refused.

use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::keychain::{Item, KeychainError};

/// Starts every derived account name; no caller's account may contain it.
const MARKER: &str = "subspace-keychain#";
/// The first bytes of a manifest: not UTF-8, and not UTF-16 either.
const MANIFEST_TAG: [u8; 2] = [0xFF, 0xDF];
const MANIFEST_PREFIX: &str = "subspace-keychain/chunks/v2/";
const ACCOUNT_DIGITS: usize = 32;
const GENERATION_DIGITS: usize = 32;
const COUNT_DIGITS: usize = 3;
const LENGTH_DIGITS: usize = 9;
const DIGEST_DIGITS: usize = 64;
/// A secret that needs only one chunk fits in one item.
const MIN_CHUNKS: usize = 2;
/// The most chunks one secret may use: 240 KiB of ASCII in Windows chunks.
const MAX_CHUNKS: usize = 256;
/// The longest derived name: the marker, the scope hash, the generation,
/// two slashes and an index of at most 3 digits.
const MAX_DERIVED_BYTES: usize = MARKER.len() + ACCOUNT_DIGITS + GENERATION_DIGITS + 5;

/// A manifest's fields, read back.
pub(super) struct Manifest {
    pub(super) generation: String,
    pub(super) count: usize,
    pub(super) length: usize,
    pub(super) digest: String,
}

/// The prefix of the account's derived names under `service`, unless the
/// account is longer than `longest` bytes or could be mistaken for one.
pub(super) fn namespace(
    service: &str,
    account: &str,
    longest: usize,
) -> Result<String, KeychainError> {
    let [lower, upper] = [
        account.to_lowercase(),
        account.to_uppercase().to_lowercase(),
    ];
    let reserved = account.contains('\0') || lower.contains(MARKER) || upper.contains(MARKER);
    if reserved || account.len() > longest {
        return Err(KeychainError);
    }
    let scope = format!(
        "{}:{}{}",
        service.len(),
        service.to_ascii_lowercase(),
        account.to_ascii_lowercase()
    );
    let mut hash = hex::encode(Sha256::digest(scope.as_bytes()));
    hash.truncate(ACCOUNT_DIGITS);
    Ok(format!("{MARKER}{hash}/"))
}

/// Fails unless every derived target name, `<chunk>.<service>`, fits in
/// `longest` bytes.
pub(super) fn room(service: &str, longest: usize) -> Result<(), KeychainError> {
    match MAX_DERIVED_BYTES + 1 + service.len() <= longest {
        true => Ok(()),
        false => Err(KeychainError),
    }
}

pub(super) fn fits(secret: &str, units: usize) -> bool {
    secret.encode_utf16().count() <= units
}

/// `secret` in pieces of at most `units` UTF-16 code units, split between
/// characters, or an error when it needs more than [`MAX_CHUNKS`].
pub(super) fn pieces(secret: &str, units: usize) -> Result<Vec<&str>, KeychainError> {
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

/// The piece in a chunk, unless it is missing, empty or another save's.
pub(super) fn piece(item: Option<Item>, header: &str) -> Result<Zeroizing<String>, KeychainError> {
    let Some(Item::Text(value)) = item else {
        return Err(KeychainError);
    };
    let value = Zeroizing::new(value);
    value
        .strip_prefix(header)
        .filter(|piece| !piece.is_empty())
        .map(|piece| Zeroizing::new(piece.to_owned()))
        .ok_or(KeychainError)
}

pub(super) fn concat(pieces: &[Zeroizing<String>]) -> Zeroizing<String> {
    let length = pieces.iter().map(|piece| piece.len()).sum();
    let mut secret = Zeroizing::new(String::with_capacity(length));
    pieces.iter().for_each(|piece| secret.push_str(piece));
    secret
}

/// Fails unless the secret has the manifest's length and digest.
pub(super) fn verify(manifest: &Manifest, digest: &str, secret: &str) -> Result<(), KeychainError> {
    match secret.len() == manifest.length && digest == manifest.digest {
        true => Ok(()),
        false => Err(KeychainError),
    }
}

/// Fails unless the `actual` pieces are the `expected` ones of a secret that
/// did not fit in one item.
pub(super) fn same(
    expected: &[&str],
    actual: &[Zeroizing<String>],
    fits: bool,
) -> Result<(), KeychainError> {
    let equal = expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| *expected == actual.as_str());
    match equal && !fits {
        true => Ok(()),
        false => Err(KeychainError),
    }
}

/// The secret, out of its wrapper; the caller now owns the only copy.
pub(super) fn release(mut secret: Zeroizing<String>) -> String {
    std::mem::take(&mut *secret)
}

pub(super) fn read_manifest(bytes: &[u8]) -> Option<Manifest> {
    let text = bytes.strip_prefix(&MANIFEST_TAG)?;
    let fields = std::str::from_utf8(text)
        .ok()?
        .strip_prefix(MANIFEST_PREFIX)?;
    let [generation, count, length, digest] = fields.split('/').collect::<Vec<_>>()[..] else {
        return None;
    };
    let hex = |field: &str, digits| {
        field.len() == digits
            && field
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    };
    let number = |field: &str, digits| {
        let decimal = field.len() == digits && field.bytes().all(|b| b.is_ascii_digit());
        decimal.then(|| field.parse::<usize>().ok()).flatten()
    };
    let count =
        number(count, COUNT_DIGITS).filter(|count| (MIN_CHUNKS..=MAX_CHUNKS).contains(count))?;
    // Every piece holds at least one byte.
    let length = number(length, LENGTH_DIGITS).filter(|length| *length >= count)?;
    (hex(generation, GENERATION_DIGITS) && hex(digest, DIGEST_DIGITS)).then(|| Manifest {
        generation: generation.to_owned(),
        count,
        length,
        digest: digest.to_owned(),
    })
}

pub(super) fn manifest(generation: &str, count: usize, length: usize, digest: &str) -> Vec<u8> {
    let text = format!(
        "{MANIFEST_PREFIX}{generation}/{count:0c$}/{length:0l$}/{digest}",
        c = COUNT_DIGITS,
        l = LENGTH_DIGITS
    );
    [&MANIFEST_TAG[..], text.as_bytes()].concat()
}

pub(super) fn digest(generation: &str, secret: &str) -> String {
    hex::encode(
        Sha256::new()
            .chain_update(generation)
            .chain_update(secret)
            .finalize(),
    )
}

pub(super) fn generation() -> String {
    format!(
        "{:0width$x}",
        rand::random::<u128>(),
        width = GENERATION_DIGITS
    )
}

pub(super) fn chunk_name(namespace: &str, generation: &str, index: usize) -> String {
    format!("{namespace}{generation}/{index}")
}

pub(super) fn chunk_header(generation: &str, index: usize) -> String {
    format!("{generation}/{index}/")
}

/// Matches, ignoring case as Windows does, exactly the target names of the
/// namespace's chunks under `service`.
pub(super) fn chunk_pattern(namespace: &str, service: &str) -> String {
    format!(
        "(?i)^{}[0-9a-f]{{{GENERATION_DIGITS}}}/[0-9]+\\.{}$",
        regex::escape(namespace),
        regex::escape(service)
    )
}

/// The account name of a found `target`, if it is exactly, case included,
/// the target name of one of the namespace's chunks under `service`.
pub(super) fn chunk_of(target: &str, namespace: &str, service: &str) -> Option<String> {
    let rest = target
        .strip_prefix(namespace)?
        .strip_suffix(service)?
        .strip_suffix('.')?;
    let (generation, index) = rest.split_once('/')?;
    let index = index
        .parse::<usize>()
        .ok()
        .filter(|index| *index < MAX_CHUNKS)?;
    let hex = generation.len() == GENERATION_DIGITS
        && generation
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let name = format!("{namespace}{generation}/{index}");
    (hex && format!("{name}.{service}") == target).then_some(name)
}
