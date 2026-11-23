//! Damaged items: never part of a secret, never another secret.

use std::collections::BTreeMap;

use super::expect::{assert_split_round_trip, digest_of, fields, namespace_of};
use super::memory::{ACCOUNT, MANIFEST_PREFIX, MAX_CHUNKS, Memory, PIECE_UNITS, Value, WINDOWS};
use crate::keychain::KeychainError;
use crate::keychain::split::{load, save};

/// A store holding a secret of 3,000 `b`s in four chunks, and its first chunk.
fn split_store() -> (Memory, String, String) {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, &"b".repeat(3_000)).unwrap();
    let (name, value) = store
        .items()
        .into_iter()
        .find(|(name, _)| name != ACCOUNT)
        .unwrap();
    let Value::Text(value) = value else {
        panic!("a chunk is text");
    };
    (store, name, value)
}

/// Writes `pieces` as chunks with a manifest of `count` and `length` and the
/// digest of the joined pieces, as a save would if it split them so.
fn plant(pieces: &[String], count: usize, length: usize) -> Memory {
    let store = Memory::windows();
    let generation = "0123456789abcdef".repeat(2);
    let namespace = namespace_of(ACCOUNT);
    for (index, piece) in pieces.iter().enumerate() {
        let chunk = format!("{generation}/{index}/{piece}");
        store.insert(
            &format!("{namespace}{generation}/{index}"),
            Value::Text(chunk),
        );
    }
    let digest = digest_of(&generation, &pieces.concat());
    let fields = format!("{generation}/{count:03}/{length:09}/{digest}");
    store.insert(
        ACCOUNT,
        Value::Bytes([MANIFEST_PREFIX, fields.as_bytes()].concat()),
    );
    store
}

#[test]
fn a_missing_truncated_empty_or_bytes_chunk_fails_closed() {
    let (store, chunk, value) = split_store();
    let (header, _) = value.split_at(value.rfind('/').unwrap() + 1);
    let damaged = [
        Value::Text(value[..value.len() - 1].to_owned()),
        Value::Text(header.to_owned()),
        Value::Text(String::new()),
        Value::Bytes(value.clone().into_bytes()),
    ];
    for damage in damaged {
        store.insert(&chunk, damage);
        assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    }
    store.remove(&chunk);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
}

#[test]
fn a_chunk_with_the_same_length_but_other_content_fails_closed() {
    let (store, chunk, value) = split_store();
    // A valid header with another payload of the same length.
    let (header, payload) = value.split_at(value.rfind('/').unwrap() + 1);
    let substituted = format!("{header}c{}", &payload[1..]);
    store.insert(&chunk, Value::Text(substituted));
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));

    // A chunk of an earlier save of the same length, under this save's name.
    let earlier = Memory::windows();
    save(&earlier, WINDOWS, ACCOUNT, &"b".repeat(3_000)).unwrap();
    let foreign = earlier
        .items()
        .into_iter()
        .find(|(name, _)| name != ACCOUNT);
    store.insert(&chunk, foreign.unwrap().1);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
}

#[test]
fn a_manifest_cut_short_reads_as_bytes_and_fails_closed() {
    let (store, _, _) = split_store();
    let Value::Bytes(manifest) = store.items()[ACCOUNT].clone() else {
        panic!("a split secret's item is bytes");
    };
    for end in 1..manifest.len() {
        // Raw bytes, read back through keyring's Windows rules.
        store.insert(ACCOUNT, Value::Bytes(manifest[..end].to_vec()));
        assert!(matches!(store.items()[ACCOUNT], Value::Bytes(_)));
        assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    }
}

#[test]
fn a_damaged_manifest_fails_closed() {
    let (store, _, _) = split_store();
    let Value::Bytes(manifest) = store.items()[ACCOUNT].clone() else {
        panic!("a split secret's item is bytes");
    };
    let good = fields(&Value::Bytes(manifest.clone())).unwrap();
    let text = |count: &str, length: &str, digest: &str| {
        let fields = format!("{}/{count}/{length}/{digest}", good.generation);
        Value::Bytes([MANIFEST_PREFIX, fields.as_bytes()].concat())
    };
    let digest = good.digest.as_str();
    let mut other = good.digest.clone().into_bytes();
    other[0] = if other[0] == b'0' { b'1' } else { b'0' };
    let damaged = [
        text("004", "000003000", &String::from_utf8(other).unwrap()),
        text("000", "000000000", digest),
        text("001", "000003000", digest),
        text(&format!("{:03}", MAX_CHUNKS + 1), "000003000", digest),
        text("004", "000000003", digest),
        text("004", "000002999", digest),
        text("4", "3000", digest),
        text("+04", "000003000", digest),
        text("004", "000003000", &digest.to_uppercase()),
        text("004", "000003000", &format!("{digest}/0")),
        Value::Bytes(manifest[2..].to_vec()),
        Value::Bytes(vec![0xff]),
    ];
    for damage in damaged {
        store.insert(ACCOUNT, damage);
        assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    }
    store.insert(ACCOUNT, Value::Bytes(manifest));
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some("b".repeat(3_000))));
}

#[test]
fn a_manifest_of_a_layout_no_save_makes_fails_closed() {
    let a = |units: usize| "a".repeat(units);
    // A save makes these pieces of 2,000 `a`s, and the planted copy loads.
    let canonical = [a(PIECE_UNITS), a(PIECE_UNITS), a(80)];
    let store = plant(&canonical, 3, 2_000);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(a(2_000))));
    let impossible = [
        // A secret that fits in one credential.
        (vec![a(50), a(50)], 2, 100),
        // Pieces that are not full, or longer than a save makes them.
        (vec![a(1_000), a(1_000)], 2, 2_000),
        (vec![a(900), a(1_020), a(80)], 3, 2_000),
        (vec![a(PIECE_UNITS), a(PIECE_UNITS), a(79), a(1)], 4, 2_000),
        // A count or a length that disagrees with the pieces.
        (canonical[..2].to_vec(), 2, 2_000),
        (canonical.to_vec(), 3, 1_999),
    ];
    for (pieces, count, length) in impossible {
        let store = plant(&pieces, count, length);
        assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    }
}

#[test]
fn a_secret_beyond_the_chunk_limit_is_refused_and_changes_nothing() {
    let store = Memory::holding(ACCOUNT, Value::Text("fixture-token".to_owned()));
    let longest = "a".repeat(MAX_CHUNKS * PIECE_UNITS);
    assert_eq!(
        save(&store, WINDOWS, ACCOUNT, &format!("{longest}a")),
        Err(KeychainError)
    );
    assert_eq!(
        store.items(),
        BTreeMap::from([(ACCOUNT.to_owned(), Value::Text("fixture-token".to_owned()))])
    );
    assert_split_round_trip(&longest);
}
