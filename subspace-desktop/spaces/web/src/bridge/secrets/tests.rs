use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use subspace_framework::core::Secret;
use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

use super::{
    BAD_KEY, BAD_VALUE, DECLARED_KEY, MAX_APP_SECRETS, Owner, SecretWrite, TOO_FAST, TOO_MANY,
    app_secret_count, check, is_app_secret, readable, written,
};
use crate::manifest::{Manifest, parse};

mod save;

const APP: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// The strict owner of an app with grant identity `identity`: no recorded
/// install source.
fn owner(identity: &str) -> Owner {
    Owner::new(
        identity,
        &WebBoardSource::Folder("/store/fleet".into()),
        None,
    )
}

fn manifest_json() -> Value {
    json!({"name": "Fleet", "version": "1", "permissions": ["secrets", "save-secrets"],
        "settings": [{"key": "token", "label": "Token", "type": "secret"},
            {"key": "endpoint", "label": "Endpoint", "type": "url"}]})
}

fn manifest() -> Manifest {
    parse(&manifest_json().to_string()).unwrap()
}

fn board(secrets: &[(&str, &str)]) -> WebBoard {
    WebBoard {
        id: "fleet".into(),
        title: "Fleet".into(),
        source: WebBoardSource::Url("https://fleet.example".into()),
        settings: BTreeMap::new(),
        secrets: secrets
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Secret::new(*value)))
            .collect(),
        permissions: BTreeMap::new(),
    }
}

fn write(key: &str, value: Option<&str>) -> SecretWrite {
    SecretWrite {
        key: key.into(),
        value: value.map(str::to_owned),
    }
}

#[test]
fn the_page_reads_declared_secrets_and_only_those_its_own_app_saved() {
    let (mine, empty) = (
        owner(APP).stored_key("fleet-a"),
        owner(APP).stored_key("empty"),
    );
    let theirs = owner(OTHER).stored_key("fleet-b");
    let board = board(&[
        ("token", "declared"),
        (&mine, "own"),
        (&empty, ""),
        (&theirs, "another app's"),
        ("fleet-c", "saved before secrets had owners"),
        ("endpoint", "legacy"),
    ]);
    assert_eq!(
        readable(&manifest(), &owner(APP), &board),
        json!({"fleet-a": "own", "token": "declared"})
    );
    assert_eq!(
        readable(&manifest(), &owner(OTHER), &board),
        json!({"fleet-b": "another app's", "token": "declared"})
    );
    assert_eq!(app_secret_count(&manifest(), &owner(APP), &board), 2);
}

#[test]
fn keys_and_values_are_checked_and_every_declared_key_refused() {
    let (manifest, board) = (manifest(), board(&[("token", "user's")]));
    let check = |write: SecretWrite| check(&manifest, &owner(APP), &board, &write);
    assert_eq!(check(write("fleet-a", Some("t"))), Ok(()));
    assert_eq!(check(write("fleet-a", None)), Ok(()));
    for declared in ["token", "endpoint"] {
        assert_eq!(check(write(declared, Some("t"))), Err(DECLARED_KEY));
        assert_eq!(check(write(declared, None)), Err(DECLARED_KEY));
    }
    assert_eq!(check(write("1fleet", Some("t"))), Err(BAD_KEY));
    assert_eq!(check(write(&"a".repeat(41), Some("t"))), Err(BAD_KEY));
    assert_eq!(check(write("a:b", Some("t"))), Err(BAD_KEY));
    assert_eq!(check(write("fleet-a", Some(""))), Err(BAD_VALUE));
    assert_eq!(check(write("fleet-a", Some("a\nb"))), Err(BAD_VALUE));
    assert_eq!(
        check(write("fleet-a", Some(&"x".repeat(4097)))),
        Err(BAD_VALUE)
    );
    assert_eq!(check(write("fleet-a", Some(&"界".repeat(4096)))), Ok(()));
}

#[test]
fn at_most_32_own_secrets_and_other_apps_and_declared_ones_do_not_count() {
    let keys: Vec<String> = (0..MAX_APP_SECRETS)
        .map(|n| owner(APP).stored_key(&format!("fleet-{n}")))
        .collect();
    let theirs = owner(OTHER).stored_key("fleet-x");
    let mut pairs: Vec<(&str, &str)> = keys.iter().map(|key| (key.as_str(), "t")).collect();
    pairs.extend([("token", "declared"), (theirs.as_str(), "t")]);
    let (manifest, board) = (manifest(), board(&pairs));
    assert_eq!(
        app_secret_count(&manifest, &owner(APP), &board),
        MAX_APP_SECRETS
    );
    let check = |identity, write: SecretWrite| check(&manifest, &owner(identity), &board, &write);
    assert_eq!(check(APP, write("fleet-new", Some("t"))), Err(TOO_MANY));
    assert_eq!(check(APP, write("fleet-0", Some("replaced"))), Ok(()));
    assert_eq!(check(APP, write("fleet-0", None)), Ok(()));
    assert_eq!(check(OTHER, write("fleet-new", Some("t"))), Ok(()));
}

#[test]
fn only_keys_a_page_saved_are_app_secrets() {
    assert!(is_app_secret(&owner(APP).stored_key("fleet-a")));
    assert!(!is_app_secret("token"));
    assert!(!is_app_secret("abc:token"));
    assert!(!is_app_secret(&format!("{}:token", "g".repeat(32))));
}

#[test]
fn saves_are_limited_to_ten_in_ten_seconds() {
    let mut saves = super::Saves::default();
    let now = Instant::now();
    for _ in 0..10 {
        assert_eq!(saves.allow(now), Ok(()));
    }
    assert_eq!(
        saves.allow(now + Duration::from_millis(9_900)),
        Err(TOO_FAST)
    );
    assert_eq!(saves.allow(now + Duration::from_secs(10)), Ok(()));
}

#[test]
fn a_write_changes_only_the_secret_and_keeps_the_entry() {
    let entry = json!({"path": "/apps/fleet", "settings": {"endpoint": "https://a.example"},
        "secrets": {"token": "t"}, "permissions": {"x.secrets": true}});
    let entry = entry.as_object().unwrap().clone();
    let forms = ["a:fleet-a".to_owned(), "b:fleet-a".to_owned()];
    let added = written(entry.clone(), &forms, Some("new"));
    assert_eq!(added["secrets"], json!({"token": "t", "a:fleet-a": "new"}));
    let replaced = written(
        added.clone(),
        &["c:fleet-a".to_owned(), "a:fleet-a".to_owned()],
        Some("v2"),
    );
    assert_eq!(
        replaced["secrets"],
        json!({"token": "t", "c:fleet-a": "v2"})
    );
    assert_eq!(added["settings"], entry["settings"]);
    assert_eq!(added["permissions"], entry["permissions"]);
    let removed = written(entry, &["token".to_owned()], None);
    assert!(!removed.contains_key("secrets"));
    assert_eq!(removed["path"], "/apps/fleet");
}
