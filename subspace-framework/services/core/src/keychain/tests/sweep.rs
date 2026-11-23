//! The sweep after a delete: best effort, and only ever this account's
//! chunks under this service.

use std::collections::BTreeSet;

use super::expect::{fields, namespace_in, namespace_of};
use super::memory::{ACCOUNT, Failure, Memory, PIECE_UNITS, SERVICE, Value, WINDOWS};
use crate::keychain::split::{delete, load, save};

/// A secret of five chunks.
fn long(fill: char) -> String {
    fill.to_string().repeat(4 * PIECE_UNITS + 1)
}

/// A store holding a long secret under [`ACCOUNT`], and its generation.
fn long_store() -> (Memory, String) {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, &long('a')).unwrap();
    let generation = fields(&store.items()[ACCOUNT]).unwrap().generation;
    (store, generation)
}

#[test]
fn a_delete_touches_nothing_of_another_service_or_another_account() {
    let (store, generation) = long_store();
    let other = store.under("x.app.example");
    save(&other, WINDOWS, ACCOUNT, &long('b')).unwrap();
    save(&store, WINDOWS, "other", &long('c')).unwrap();
    let namespace = namespace_of(ACCOUNT);
    let foreign = [
        // Credentials no save writes, whose names resemble this account's chunks.
        format!("{namespace}{}/0.{SERVICE}", generation.to_uppercase()),
        format!("{namespace}{generation}/00.{SERVICE}"),
        format!("{namespace}{generation}/0.{SERVICE}.extra"),
        format!("x{namespace}{generation}/0.{SERVICE}"),
        format!("{namespace}{generation}/0/1.{SERVICE}"),
        format!("{namespace}{generation}/0.example"),
        format!(
            "{}{generation}/0.{SERVICE}",
            namespace_in("x.app.example", ACCOUNT)
        ),
    ];
    for target in &foreign {
        store.insert_target(target, Value::Text("fixture-foreign".to_owned()));
    }
    let before = store.targets();
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    let removed: BTreeSet<String> = before.difference(&store.targets()).cloned().collect();
    let ours: BTreeSet<String> = (0..5)
        .map(|index| format!("{namespace}{generation}/{index}.{SERVICE}"))
        .chain([format!("{ACCOUNT}.{SERVICE}")])
        .collect();
    assert_eq!(removed, ours);
    assert!(
        foreign
            .iter()
            .all(|target| store.targets().contains(target))
    );
    assert_eq!(load(&other, WINDOWS, ACCOUNT), Ok(Some(long('b'))));
    assert_eq!(load(&store, WINDOWS, "other"), Ok(Some(long('c'))));
}

#[test]
fn a_delete_succeeds_when_the_search_fails() {
    // Delete the account's item, read it again, then search.
    for secret in ["fixture-short".to_owned(), long('a')] {
        let store = Memory::windows();
        save(&store, WINDOWS, ACCOUNT, &secret).unwrap();
        store.fail_step(3, Failure::Error);
        assert_eq!(delete(&store, WINDOWS, ACCOUNT), Ok(()));
        assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(None));
        // The chunks wait for the next sign-out.
        delete(&store, WINDOWS, ACCOUNT).unwrap();
        assert!(store.names().is_empty());
    }
}

#[test]
fn a_delete_skips_what_it_cannot_parse_or_delete_and_sweeps_the_rest() {
    let (store, generation) = long_store();
    let namespace = namespace_of(ACCOUNT);
    for stray in [
        String::new(),
        "garbage".to_owned(),
        format!("{namespace}{generation}/x.{SERVICE}"),
        format!("{namespace}{generation}/9.{SERVICE}"),
    ] {
        store.stray(&stray);
    }
    // The first of the five chunk deletions fails.
    store.fail_step(4, Failure::Error);
    assert_eq!(delete(&store, WINDOWS, ACCOUNT), Ok(()));
    assert_eq!(store.names().len(), 1);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(None));
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    assert!(store.names().is_empty());
}

#[test]
fn a_delete_keeps_the_chunks_of_a_secret_saved_after_it_began() {
    let (store, _) = long_store();
    // Another process saves just before the sweep reads the account's item.
    store.before_step(2, |store| {
        save(store, WINDOWS, ACCOUNT, &long('b')).unwrap();
    });
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(long('b'))));
    assert_eq!(store.names().len(), 1 + 5);
}
