//! Damaged items and saves that fail: never part of a secret, never a mix.

use std::collections::BTreeMap;

use super::memory::{
    ACCOUNT, MANIFEST_PREFIX, MAX_CHUNKS, Memory, PIECE_UNITS, WINDOWS, assert_split_round_trip,
};
use crate::keychain::KeychainError;
use crate::keychain::split::{load, save};

#[test]
fn a_missing_truncated_or_foreign_chunk_fails_closed() {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, &"a".repeat(3_000)).unwrap();
    let earlier = store.items();
    save(&store, WINDOWS, ACCOUNT, &"b".repeat(3_000)).unwrap();
    let (chunk, value) = store
        .items()
        .into_iter()
        .find(|(name, _)| name != ACCOUNT)
        .unwrap();
    let foreign = earlier.into_iter().find(|(name, _)| name != ACCOUNT);
    store.insert(&chunk, &foreign.unwrap().1);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    store.insert(&chunk, &value[..value.len() - 1]);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    store.remove(&chunk);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
}

#[test]
fn an_unreadable_manifest_fails_closed() {
    let generation = "0".repeat(32);
    let values = [
        format!("subspace-keychain/chunks/v2/{generation}/2/10"),
        "subspace-keychain/chunks/v1/not-a-generation/2/10".to_owned(),
        format!("{MANIFEST_PREFIX}{generation}/0/0"),
        format!("{MANIFEST_PREFIX}{generation}/{}/10", MAX_CHUNKS + 1),
        format!("{MANIFEST_PREFIX}{generation}/2"),
        format!("{MANIFEST_PREFIX}{generation}/2/10/1"),
        "subspace-keychain/".to_owned(),
    ];
    for value in &values {
        let store = Memory::holding(ACCOUNT, value);
        assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    }
}

#[test]
fn a_save_that_stops_part_way_keeps_the_previous_secret() {
    // Five chunks and the manifest.
    let next = "n".repeat(4 * PIECE_UNITS + 1);
    for previous in ["fixture-token".to_owned(), "p".repeat(3_000)] {
        for write in 1..=6 {
            let store = Memory::windows();
            save(&store, WINDOWS, ACCOUNT, &previous).unwrap();
            store.fail_write(write, true);
            assert_eq!(save(&store, WINDOWS, ACCOUNT, &next), Err(KeychainError));
            store.recover();
            assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(previous.clone())));
            save(&store, WINDOWS, ACCOUNT, &next).unwrap();
            assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(next.clone())));
        }
    }
}

#[test]
fn a_failed_save_removes_its_own_chunks() {
    let next = "n".repeat(4 * PIECE_UNITS + 1);
    for previous in ["fixture-token".to_owned(), "p".repeat(3_000)] {
        for write in 1..=6 {
            let store = Memory::windows();
            save(&store, WINDOWS, ACCOUNT, &previous).unwrap();
            let before = store.items();
            store.fail_write(write, false);
            assert_eq!(save(&store, WINDOWS, ACCOUNT, &next), Err(KeychainError));
            assert_eq!(store.items(), before);
        }
    }
}

#[test]
fn a_secret_beyond_the_chunk_limit_is_refused_and_changes_nothing() {
    let store = Memory::holding(ACCOUNT, "fixture-token");
    let longest = "a".repeat(MAX_CHUNKS * PIECE_UNITS);
    assert_eq!(
        save(&store, WINDOWS, ACCOUNT, &format!("{longest}a")),
        Err(KeychainError)
    );
    assert_eq!(
        store.items(),
        BTreeMap::from([(ACCOUNT.to_owned(), "fixture-token".to_owned())])
    );
    assert_split_round_trip(&longest);
}
