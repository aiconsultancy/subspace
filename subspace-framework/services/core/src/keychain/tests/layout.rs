//! Where each secret is stored, and that it loads back.

use std::collections::{BTreeMap, BTreeSet};

use super::memory::{
    ACCOUNT, ITEM_UNITS, MANIFEST_PREFIX, Memory, PIECE_UNITS, WINDOWS, assert_no_orphans,
    assert_split_round_trip,
};
use crate::keychain::split::{delete, load, save};

#[test]
fn a_secret_that_fits_one_credential_is_stored_as_before() {
    let secrets = [
        String::new(),
        "fixture-token".to_owned(),
        "a".repeat(ITEM_UNITS),
        "\u{1d11e}".repeat(ITEM_UNITS / 2),
    ];
    for secret in &secrets {
        let store = Memory::windows();
        save(&store, WINDOWS, ACCOUNT, secret).unwrap();
        assert_eq!(
            store.items(),
            BTreeMap::from([(ACCOUNT.to_owned(), secret.clone())])
        );
        assert_eq!(
            load(&store, WINDOWS, ACCOUNT).unwrap().as_ref(),
            Some(secret)
        );
    }
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
fn a_split_secret_is_a_manifest_and_numbered_chunks() {
    let store = Memory::windows();
    let secret = "a".repeat(2 * PIECE_UNITS + 80);
    save(&store, WINDOWS, ACCOUNT, &secret).unwrap();
    let items = store.items();
    let fields = items[ACCOUNT].strip_prefix(MANIFEST_PREFIX).unwrap();
    let [generation, count, bytes] = fields.split('/').collect::<Vec<_>>()[..] else {
        panic!("a manifest has three fields");
    };
    assert_eq!(generation.len(), 32);
    assert!(
        generation
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_eq!((count, bytes), ("3", "2000"));
    let pieces: Vec<&str> = (0..3)
        .map(|index| {
            let chunk = &items[&format!("{ACCOUNT}#chunk/{generation}/{index}")];
            chunk
                .strip_prefix(&format!("{generation}/{index}/"))
                .unwrap()
        })
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
    ];
    for secret in &sequence {
        save(&store, WINDOWS, ACCOUNT, secret).unwrap();
        assert_eq!(
            load(&store, WINDOWS, ACCOUNT).unwrap().as_ref(),
            Some(secret)
        );
        assert_no_orphans(&store);
    }
    assert_eq!(store.names(), BTreeSet::from([ACCOUNT.to_owned()]));
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
fn a_single_item_from_an_earlier_build_still_loads() {
    let legacy = r#"{"access_token":"fixture-access","refresh_token":"fixture-refresh"}"#;
    let store = Memory::holding(ACCOUNT, legacy);
    assert_eq!(
        load(&store, WINDOWS, ACCOUNT).unwrap().as_deref(),
        Some(legacy)
    );
    save(&store, WINDOWS, ACCOUNT, &"a".repeat(3_000)).unwrap();
    assert_no_orphans(&store);
}

#[test]
fn a_secret_that_looks_like_a_manifest_round_trips() {
    let generation = "0".repeat(32);
    let secrets = [
        format!("{MANIFEST_PREFIX}{generation}/2/10"),
        "subspace-keychain/plain/fixture".to_owned(),
        "subspace-keychain/".to_owned(),
        format!("{MANIFEST_PREFIX}{generation}/2/10{}", "a".repeat(3_000)),
    ];
    for secret in &secrets {
        let store = Memory::windows();
        save(&store, WINDOWS, ACCOUNT, secret).unwrap();
        assert_eq!(
            load(&store, WINDOWS, ACCOUNT).unwrap().as_ref(),
            Some(secret)
        );
        delete(&store, WINDOWS, ACCOUNT).unwrap();
        assert!(store.names().is_empty());
    }
}

#[test]
fn without_a_limit_every_secret_is_one_item_as_before() {
    let store = Memory::default();
    let secrets = [
        "a".repeat(64 * 1024),
        format!("{MANIFEST_PREFIX}{}/2/10", "0".repeat(32)),
        "subspace-keychain/plain/fixture".to_owned(),
    ];
    for secret in &secrets {
        save(&store, None, ACCOUNT, secret).unwrap();
        assert_eq!(
            store.items(),
            BTreeMap::from([(ACCOUNT.to_owned(), secret.clone())])
        );
        assert_eq!(load(&store, None, ACCOUNT).unwrap().as_ref(), Some(secret));
    }
    delete(&store, None, ACCOUNT).unwrap();
    assert!(store.names().is_empty());
}
