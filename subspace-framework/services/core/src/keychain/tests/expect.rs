//! What the stored format says an item holds, and checks on a store.

use std::collections::BTreeSet;

use sha2::{Digest as _, Sha256};

use super::memory::{
    ACCOUNT, HEADER_UNITS, MANIFEST_PREFIX, Memory, PIECE_UNITS, SERVICE, Value, WINDOWS,
};
use crate::keychain::KeychainError;
use crate::keychain::split::{load, save};

/// The prefix of an account's derived names under a service, computed as
/// the format says.
pub(super) fn namespace_in(service: &str, account: &str) -> String {
    let scope = format!(
        "{}:{}{}",
        service.len(),
        service.to_ascii_lowercase(),
        account.to_ascii_lowercase()
    );
    let hash = hex::encode(Sha256::digest(scope.as_bytes()));
    format!("subspace-keychain#{}/", &hash[..32])
}

pub(super) fn namespace_of(account: &str) -> String {
    namespace_in(SERVICE, account)
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

/// The digest a manifest holds for `secret` saved as `generation`.
pub(super) fn digest_of(generation: &str, secret: &str) -> String {
    let digest = Sha256::new()
        .chain_update(generation)
        .chain_update(secret)
        .finalize();
    hex::encode(digest)
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
