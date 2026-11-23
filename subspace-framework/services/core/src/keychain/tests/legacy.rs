//! Items written by earlier builds, and what earlier builds read.

use super::expect::{namespace_of, old_build_load};
use super::memory::{ACCOUNT, ITEM_UNITS, Memory, Value, WINDOWS};
use crate::keychain::KeychainError;
use crate::keychain::split::{delete, load, save};

/// Text that looks like this format, or like its unreleased first draft.
fn lookalikes() -> Vec<String> {
    let generation = "0".repeat(32);
    let digest = "0".repeat(64);
    vec![
        r#"{"access_token":"fixture-access","refresh_token":"fixture-refresh"}"#.to_owned(),
        format!("subspace-keychain/chunks/v2/{generation}/002/000000010/{digest}"),
        format!("\u{ff}\u{df}subspace-keychain/chunks/v2/{generation}/002/000000010/{digest}"),
        format!("subspace-keychain/chunks/v1/{generation}/2/10"),
        "subspace-keychain/plain/fixture".to_owned(),
        "subspace-keychain/".to_owned(),
        format!("{}{generation}/0", namespace_of(ACCOUNT)),
        format!("{generation}/0/fixture"),
        "\u{ff}subspace-keychain/chunks/v2/".to_owned(),
        "a".repeat(ITEM_UNITS),
    ]
}

#[test]
fn text_from_an_earlier_build_loads_verbatim() {
    for value in lookalikes() {
        let store = Memory::holding(ACCOUNT, Value::Text(value.clone()));
        assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(value)));
    }
}

#[test]
fn an_earlier_build_reads_every_secret_that_fits_verbatim() {
    for secret in lookalikes() {
        let store = Memory::windows();
        save(&store, WINDOWS, ACCOUNT, &secret).unwrap();
        assert_eq!(store.names().len(), 1);
        assert_eq!(old_build_load(&store, ACCOUNT), Ok(Some(secret)));
    }
}

#[test]
fn an_earlier_build_reads_a_split_secret_as_unavailable() {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, &"a".repeat(3_000)).unwrap();
    assert_eq!(old_build_load(&store, ACCOUNT), Err(KeychainError));
}

#[test]
fn what_an_earlier_build_writes_over_a_split_secret_wins() {
    // It saves a secret that fits, as text over the manifest.
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, &"a".repeat(3_000)).unwrap();
    store.insert(ACCOUNT, Value::Text("fixture-token".to_owned()));
    assert_eq!(
        load(&store, WINDOWS, ACCOUNT),
        Ok(Some("fixture-token".to_owned()))
    );
    // A save never searches, so the chunks it replaced stay until a sign-out.
    save(&store, WINDOWS, ACCOUNT, "fixture-next").unwrap();
    assert_eq!(store.names().len(), 5);
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    assert!(store.names().is_empty());

    // It signs out, deleting the manifest but not the chunks.
    save(&store, WINDOWS, ACCOUNT, &"b".repeat(3_000)).unwrap();
    store.remove(ACCOUNT);
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(None));
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    assert!(store.names().is_empty());
}
