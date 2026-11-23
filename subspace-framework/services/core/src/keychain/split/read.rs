//! Reading a split secret back: its chunks, checked against the manifest
//! and against the pieces a save would make.

use zeroize::Zeroizing;

use super::Limits;
use super::format::{
    Manifest, chunk_header, chunk_name, concat, digest, fits, piece, pieces, read_manifest,
    release, same, verify,
};
use crate::keychain::{Item, KeychainError, Store};

pub(super) fn decode(
    store: &impl Store,
    namespace: &str,
    limits: Limits,
    item: Item,
) -> Result<String, KeychainError> {
    match item {
        Item::Text(secret) => Ok(secret),
        Item::Bytes(bytes) => join(store, namespace, limits, read_manifest(&bytes)),
    }
}

fn join(
    store: &impl Store,
    namespace: &str,
    limits: Limits,
    manifest: Option<Manifest>,
) -> Result<String, KeychainError> {
    let manifest = manifest.ok_or(KeychainError)?;
    let pieces = (0..manifest.count)
        .map(|index| read_chunk(store, namespace, &manifest, index))
        .collect::<Result<Vec<_>, _>>()?;
    let secret = concat(&pieces);
    verify(&manifest, &digest(&manifest.generation, &secret), &secret)?;
    canonical(&secret, &pieces, limits)?;
    Ok(release(secret))
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

/// Fails unless the secret needed splitting and the `read` pieces are
/// exactly the ones a save makes of it.
fn canonical(
    secret: &str,
    read: &[Zeroizing<String>],
    limits: Limits,
) -> Result<(), KeychainError> {
    let expected = pieces(secret, limits.piece)?;
    same(&expected, read, fits(secret, limits.item))
}
