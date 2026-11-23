//! Where each secret is stored, and that it loads back.

use std::collections::{BTreeMap, BTreeSet};

use super::expect::{assert_no_orphans, assert_split_round_trip, digest_of, fields, namespace_of};
use super::memory::{
    ACCOUNT, ACCOUNT_BYTES, BLOB_BYTES, HEADER_UNITS, ITEM_UNITS, MANIFEST_PREFIX, MAX_CHUNKS,
    Memory, PIECE_UNITS, TARGET_BYTES, Value, WINDOWS, classify,
};
use crate::keychain::split::{delete, load, save};
use crate::keychain::{KeychainError, Store};

#[test]
fn a_secret_that_fits_one_credential_is_stored_as_text_as_before() {
    let secrets = [
        String::new(),
        "fixture-token".to_owned(),
        "a".repeat(ITEM_UNITS),
        "\u{1d11e}".repeat(ITEM_UNITS / 2),
        format!("{}é", "a".repeat(ITEM_UNITS - 1)),
    ];
    for secret in &secrets {
        let store = Memory::windows();
        save(&store, WINDOWS, ACCOUNT, secret).unwrap();
        assert_eq!(
            store.items(),
            BTreeMap::from([(ACCOUNT.to_owned(), Value::Text(secret.clone()))])
        );
        assert_eq!(
            load(&store, WINDOWS, ACCOUNT).unwrap().as_ref(),
            Some(secret)
        );
    }
    assert_split_round_trip(&"a".repeat(ITEM_UNITS + 1));
    assert_split_round_trip(&format!("a{}", "\u{1d11e}".repeat(ITEM_UNITS / 2)));
}

#[test]
fn a_longer_secret_is_split_into_chunks_that_load_back() {
    for units in [ITEM_UNITS + 1, 1_919, 1_920, 1_921, 2_100, 2_881, 64 * 1024] {
        assert_split_round_trip(&"a".repeat(units));
        assert_split_round_trip(&"é€".repeat(units / 2 + 1));
        assert_split_round_trip(&format!("a{}", "\u{1d11e}".repeat(units / 2)));
        assert_split_round_trip(&"aé€\u{1d11e}".repeat(units / 5 + 1));
    }
    // 64 KiB of UTF-8 in four-byte characters.
    assert_split_round_trip(&"\u{1d11e}".repeat(16 * 1024));
}

#[test]
fn the_test_store_refuses_exactly_what_windows_refuses() {
    let store = Memory::windows();
    assert_eq!(store.set_text("a", &"a".repeat(ITEM_UNITS)), Ok(()));
    assert_eq!(
        store.set_text("a", &"a".repeat(ITEM_UNITS + 1)),
        Err(KeychainError)
    );
    assert_eq!(store.set_bytes("b", &vec![1; BLOB_BYTES]), Ok(()));
    assert_eq!(
        store.set_bytes("b", &vec![1; BLOB_BYTES + 1]),
        Err(KeychainError)
    );
    assert_eq!(store.set_text(&"c".repeat(ACCOUNT_BYTES), "x"), Ok(()));
    assert_eq!(
        store.set_text(&"c".repeat(ACCOUNT_BYTES + 1), "x"),
        Err(KeychainError)
    );
    // `c.` and the service make the target name.
    let longest = store.under(&"s".repeat(TARGET_BYTES - 2));
    assert_eq!(longest.set_text("c", "x"), Ok(()));
    let longer = store.under(&"s".repeat(TARGET_BYTES - 1));
    assert_eq!(longer.set_text("c", "x"), Err(KeychainError));
}

#[test]
fn the_largest_chunk_is_its_longest_header_and_a_full_piece() {
    let store = Memory::windows();
    save(
        &store,
        WINDOWS,
        ACCOUNT,
        &"a".repeat(MAX_CHUNKS * PIECE_UNITS),
    )
    .unwrap();
    let longest = store
        .items()
        .into_iter()
        .filter(|(name, _)| name != ACCOUNT)
        .map(|(_, value)| match value {
            Value::Text(chunk) => chunk.encode_utf16().count(),
            Value::Bytes(_) => usize::MAX,
        })
        .max();
    assert_eq!(longest, Some(HEADER_UNITS + PIECE_UNITS));
}

// The largest chunk stays more than a fifth under one credential.
const _: () = assert!(HEADER_UNITS + PIECE_UNITS < ITEM_UNITS * 4 / 5);

#[test]
fn a_split_secret_is_a_manifest_in_bytes_and_numbered_chunks() {
    let store = Memory::windows();
    let secret = "a".repeat(2 * PIECE_UNITS + 80);
    save(&store, WINDOWS, ACCOUNT, &secret).unwrap();
    let items = store.items();
    let Value::Bytes(manifest) = &items[ACCOUNT] else {
        panic!("a split secret's item is bytes");
    };
    assert_eq!(manifest.len(), 141);
    assert!(manifest.starts_with(MANIFEST_PREFIX));
    assert!(String::from_utf8(manifest.clone()).is_err());
    // Not UTF-16 either, at any length.
    for end in 1..=manifest.len() {
        assert!(matches!(classify(&manifest[..end]), Value::Bytes(_)));
    }
    let fields = fields(&items[ACCOUNT]).unwrap();
    let generation = &fields.generation;
    assert_eq!(generation.len(), 32);
    assert!(
        generation
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_eq!((fields.count, fields.length), (3, 2_000));
    assert_eq!(fields.digest, digest_of(generation, &secret));
    let namespace = namespace_of(ACCOUNT);
    let pieces: Vec<String> = (0..3)
        .map(
            |index| match &items[&format!("{namespace}{generation}/{index}")] {
                Value::Text(chunk) => chunk
                    .strip_prefix(&format!("{generation}/{index}/"))
                    .unwrap()
                    .to_owned(),
                Value::Bytes(_) => panic!("a chunk is text"),
            },
        )
        .collect();
    assert_eq!(pieces.concat(), secret);
    assert_eq!(items.len(), 4);
}

#[test]
fn overwriting_a_secret_leaves_no_chunks_of_the_previous_one() {
    let store = Memory::windows();
    let sequence = [
        "a".repeat(5_000),
        "b".repeat(9_000),
        "fixture-token".to_owned(),
        "c".repeat(3_000),
        "d".repeat(ITEM_UNITS),
        "e".repeat(ITEM_UNITS + 1),
    ];
    for secret in &sequence {
        save(&store, WINDOWS, ACCOUNT, secret).unwrap();
        assert_eq!(
            load(&store, WINDOWS, ACCOUNT).unwrap().as_ref(),
            Some(secret)
        );
        assert_no_orphans(&store, ACCOUNT);
    }
}

#[test]
fn delete_removes_the_manifest_and_every_chunk() {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, &"a".repeat(5_000)).unwrap();
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    assert!(store.names().is_empty());
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(None));
    assert_eq!(delete(&store, WINDOWS, ACCOUNT), Ok(()));

    save(&store, WINDOWS, ACCOUNT, &"a".repeat(5_000)).unwrap();
    let chunk = store.names().into_iter().find(|name| name != ACCOUNT);
    store.remove(&chunk.unwrap());
    assert_eq!(delete(&store, WINDOWS, ACCOUNT), Ok(()));
    assert!(store.names().is_empty());
}

#[test]
fn an_accounts_chunks_are_its_own() {
    let store = Memory::windows();
    save(&store, WINDOWS, "one", &"a".repeat(3_000)).unwrap();
    save(&store, WINDOWS, "two", &"b".repeat(3_000)).unwrap();
    delete(&store, WINDOWS, "one").unwrap();
    assert_eq!(
        load(&store, WINDOWS, "two").unwrap(),
        Some("b".repeat(3_000))
    );
    assert_no_orphans(&store, "two");
}

#[test]
fn without_a_limit_every_secret_is_one_item_of_text_as_before() {
    let store = Memory::default();
    let secrets = [
        "a".repeat(64 * 1024),
        "subspace-keychain/chunks/v2/fixture".to_owned(),
        format!("{}fixture", namespace_of(ACCOUNT)),
    ];
    for secret in &secrets {
        save(&store, None, ACCOUNT, secret).unwrap();
        assert_eq!(
            store.items(),
            BTreeMap::from([(ACCOUNT.to_owned(), Value::Text(secret.clone()))])
        );
        assert_eq!(load(&store, None, ACCOUNT).unwrap().as_ref(), Some(secret));
    }
    store.insert(ACCOUNT, Value::Bytes(vec![0xff]));
    assert_eq!(load(&store, None, ACCOUNT), Err(KeychainError));
    delete(&store, None, ACCOUNT).unwrap();
    assert_eq!(store.names(), BTreeSet::new());
}
