//! Account and service names: refused when they could alias a derived name,
//! bounded when they are long.

use super::expect::{assert_no_orphans, namespace_in, namespace_of};
use super::memory::{ACCOUNT, ACCOUNT_BYTES, Memory, TARGET_BYTES, Value, WINDOWS};
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
    let names = store.names();
    let mut chunks = names.iter().filter(|name| **name != account);
    assert!(chunks.all(|name| name.len() <= 87));
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
    assert!(store.targets().is_empty());
}

#[test]
fn a_service_too_long_for_chunk_names_fails_before_any_write() {
    // `session.` and the service fit; a derived name and the service do not.
    let store = Memory::windows().under(&"s".repeat(TARGET_BYTES - ACCOUNT.len() - 1));
    assert_eq!(
        save(&store, WINDOWS, ACCOUNT, &"b".repeat(3_000)),
        Err(KeychainError)
    );
    assert!(store.targets().is_empty());
    save(&store, WINDOWS, ACCOUNT, "fixture-short").unwrap();
    assert_eq!(
        load(&store, WINDOWS, ACCOUNT),
        Ok(Some("fixture-short".to_owned()))
    );
}

#[test]
fn only_ascii_case_is_folded_into_the_namespace() {
    // Windows compares names without case, so ASCII case variants are one
    // credential and share their chunks. Variants in the case of another
    // letter do not, and are not supported for split secrets.
    assert_eq!(namespace_of("Session"), namespace_of("session"));
    assert_ne!(namespace_of("Ésession"), namespace_of("ésession"));
    let store = Memory::holding("Session", Value::Text("old".to_owned()));
    save(&store, WINDOWS, "Session", &"a".repeat(3_000)).unwrap();
    let namespace = namespace_of("session");
    let names = store.names();
    let mut chunks = names.iter().filter(|name| *name != "Session");
    assert!(chunks.all(|name| name.starts_with(&namespace)));
    assert_eq!(names.len(), 5);
}

#[test]
fn each_service_has_its_own_namespace() {
    assert_ne!(
        namespace_in("app.example", ACCOUNT),
        namespace_in("x.app.example", ACCOUNT)
    );
    // The length prefix keeps the service and the account apart.
    assert_ne!(namespace_in("ab", "c"), namespace_in("a", "bc"));
}

#[test]
fn an_account_saved_in_one_case_loads_and_deletes_in_another() {
    let store = Memory::windows();
    save(&store, WINDOWS, "Session", &"a".repeat(3_000)).unwrap();
    assert_eq!(
        load(&store, WINDOWS, "session"),
        Ok(Some("a".repeat(3_000)))
    );
    delete(&store, WINDOWS, "SESSION").unwrap();
    assert!(store.targets().is_empty());
}

#[test]
fn a_service_with_a_nul_is_refused_before_any_step() {
    let store = Memory::windows().under("app\0example");
    assert_eq!(
        save(&store, WINDOWS, ACCOUNT, &"a".repeat(3_000)),
        Err(KeychainError)
    );
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    assert_eq!(delete(&store, WINDOWS, ACCOUNT), Err(KeychainError));
    assert_eq!(store.steps(), 0);
}
