//! The stored format of a split secret.
//!
//! The manifest, in the account's own item, is the byte `0xFF`, never part
//! of UTF-8 text, then
//! `subspace-keychain/chunks/v2/<generation>/<count>/<length>/<digest>`: 141
//! bytes in all, and an odd length is never UTF-16 text either, so Windows
//! hands it back as bytes too. The generation is 32 random hexadecimal
//! digits, new for every save; `count` (3 digits) and `length` (10 digits,
//! the secret's UTF-8 length) are zero-padded; the digest is the SHA-256 of
//! the generation followed by the secret, in hexadecimal.
//!
//! Chunk `index` is the text `<generation>/<index>/` and its piece of the
//! secret, under the derived account name
//! `subspace-keychain#<account hash>/<generation>/<index>`, where the account
//! hash is the first 32 hexadecimal digits of the SHA-256 of the account in
//! lower case, as Windows compares names without case. Derived names are
//! therefore 87 bytes at most, whatever the account. An account containing
//! the marker, in any case, or a NUL is refused.

use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::keychain::{Item, KeychainError};

/// Starts every derived account name; no caller's account may contain it.
const MARKER: &str = "subspace-keychain#";
/// The first byte of a manifest, which never starts UTF-8 text.
const MANIFEST_TAG: u8 = 0xFF;
const MANIFEST_PREFIX: &str = "subspace-keychain/chunks/v2/";
const ACCOUNT_DIGITS: usize = 32;
const GENERATION_DIGITS: usize = 32;
const COUNT_DIGITS: usize = 3;
const LENGTH_DIGITS: usize = 10;
const DIGEST_DIGITS: usize = 64;
/// A secret that needs only one chunk fits in one item.
const MIN_CHUNKS: usize = 2;
/// The most chunks one secret may use: 240 KiB of ASCII in Windows chunks.
const MAX_CHUNKS: usize = 256;

/// A manifest's fields, read back.
pub(super) struct Manifest {
    pub(super) generation: String,
    pub(super) count: usize,
    pub(super) length: usize,
    pub(super) digest: String,
}

/// The prefix of the account's derived names, unless the account is longer
/// than `longest` bytes or could be mistaken for one of them.
pub(super) fn namespace(account: &str, longest: usize) -> Result<String, KeychainError> {
    let [lower, upper] = [
        account.to_lowercase(),
        account.to_uppercase().to_lowercase(),
    ];
    let reserved = account.contains('\0') || lower.contains(MARKER) || upper.contains(MARKER);
    if reserved || account.len() > longest {
        return Err(KeychainError);
    }
    let mut hash = hex::encode(Sha256::digest(lower.as_bytes()));
    hash.truncate(ACCOUNT_DIGITS);
    Ok(format!("{MARKER}{hash}/"))
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

/// The secret, if it has the manifest's length and digest.
pub(super) fn verify(
    manifest: &Manifest,
    mut secret: Zeroizing<String>,
    digest: &str,
) -> Result<String, KeychainError> {
    match secret.len() == manifest.length && digest == manifest.digest {
        true => Ok(std::mem::take(&mut *secret)),
        false => Err(KeychainError),
    }
}

pub(super) fn read_manifest(bytes: &[u8]) -> Option<Manifest> {
    let (&MANIFEST_TAG, text) = bytes.split_first()? else {
        return None;
    };
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
    [&[MANIFEST_TAG][..], text.as_bytes()].concat()
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
