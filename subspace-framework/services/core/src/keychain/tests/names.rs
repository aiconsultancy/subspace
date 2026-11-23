//! Account names: refused when they could alias a derived name, bounded
//! when they are long.

use super::memory::{ACCOUNT_BYTES, Memory, Value, WINDOWS, assert_no_orphans, namespace_of};
use crate::keychain::KeychainError;
use crate::keychain::split::{delete, load, save};

#[test]
fn an_account_that_could_alias_a_derived_name_is_refused_before_any_step() {
    let refused = [
        "session\0".to_owned(),
        "\0".to_owned(),
        "subspace-keychain#".to_owned(),
        "x-SubSpace-Keychain#y".to_owned(),
        "SUBSPACE-KEYCHAIN#".to_owned(),
        // The Kelvin sign is a K in lower case.
        "subspace-\u{212a}eychain#".to_owned(),
        format!("{}00/0", namespace_of("session")),
    ];
    for account in &refused {
        let store = Memory::windows();
        assert_eq!(
            save(&store, WINDOWS, account, &"a".repeat(3_000)),
            Err(KeychainError)
        );
        assert_eq!(save(&store, WINDOWS, account, "short"), Err(KeychainError));
        assert_eq!(load(&store, WINDOWS, account), Err(KeychainError));
        assert_eq!(delete(&store, WINDOWS, account), Err(KeychainError));
        assert_eq!(store.steps(), 0);
    }
}

#[test]
fn every_kind_of_caller_account_keeps_a_long_secret() {
    for account in ["session", "octocat-dev", "fleet-api:fleet-01.example"] {
        let store = Memory::windows();
        let secret = format!("{account}-{}", "a".repeat(3_000));
        save(&store, WINDOWS, account, &secret).unwrap();
        assert_eq!(load(&store, WINDOWS, account), Ok(Some(secret)));
        assert_no_orphans(&store, account);
        delete(&store, WINDOWS, account).unwrap();
        assert!(store.names().is_empty());
    }
}

#[test]
fn derived_names_stay_short_for_the_longest_account() {
    let account = "a".repeat(ACCOUNT_BYTES);
    let store = Memory::windows();
    save(&store, WINDOWS, &account, &"b".repeat(3_000)).unwrap();
    assert_eq!(load(&store, WINDOWS, &account), Ok(Some("b".repeat(3_000))));
    assert!(
        store
            .names()
            .iter()
            .filter(|name| **name != account)
            .all(|name| name.len() <= 87)
    );
}

#[test]
fn a_longer_account_fails_before_any_step() {
    let account = "a".repeat(ACCOUNT_BYTES + 1);
    let store = Memory::windows();
    assert_eq!(
        save(&store, WINDOWS, &account, &"b".repeat(3_000)),
        Err(KeychainError)
    );
    assert_eq!(store.steps(), 0);
    assert!(store.names().is_empty());
}

#[test]
fn accounts_that_differ_only_in_case_share_one_namespace() {
    // Windows compares credential names without case, so these accounts are
    // one credential, and their chunks must be one set too.
    let store = Memory::holding("Session", Value::Text("old".to_owned()));
    save(&store, WINDOWS, "Session", &"a".repeat(3_000)).unwrap();
    let namespace = namespace_of("session");
    let names = store.names();
    let mut chunks = names.iter().filter(|name| *name != "Session");
    assert!(chunks.all(|name| name.starts_with(&namespace)));
    assert_eq!(names.len(), 5);
}
