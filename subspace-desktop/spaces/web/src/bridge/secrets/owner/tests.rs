use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value, json};
use subspace_framework::core::Secret;
use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

use super::{Owner, bind_unowned, recorded};
use crate::bridge::secrets::readable;
use crate::install::{ManagedSource, RepositoryRef};
use crate::manifest::{Manifest, parse};

/// Two installs of one app: each snapshot has its own folder and manifest.
const FIRST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const SECOND: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

fn folder(name: &str) -> WebBoardSource {
    WebBoardSource::Folder(PathBuf::from("/store").join(name))
}

fn repository(repository: &str, board_path: &str, commit: &str) -> ManagedSource {
    ManagedSource::Repository {
        repository: repository.into(),
        reference: RepositoryRef::DefaultBranch,
        board_path: board_path.into(),
        commit: commit.into(),
        content: commit.into(),
    }
}

fn zip(content: &str) -> ManagedSource {
    ManagedSource::Zip {
        original: "/downloads/fleet.zip".into(),
        content: content.into(),
    }
}

fn manifest() -> Manifest {
    parse(
        &json!({"name": "Fleet", "version": "1", "permissions": ["secrets"],
            "settings": [{"key": "token", "label": "Token", "type": "secret"}]})
        .to_string(),
    )
    .unwrap()
}

fn board(secrets: &Map<String, Value>) -> WebBoard {
    WebBoard {
        id: "fleet".into(),
        title: "Fleet".into(),
        source: folder("any"),
        settings: BTreeMap::new(),
        secrets: secrets
            .iter()
            .map(|(key, value)| (key.clone(), Secret::new(value.as_str().unwrap())))
            .collect(),
        permissions: BTreeMap::new(),
    }
}

/// Whether `left` and `right` store a page's secrets under the same owner.
fn same(left: &Owner, right: &Owner) -> bool {
    left.stored_key("k") == right.stored_key("k")
}

fn reads(owner: &Owner, secrets: &Map<String, Value>) -> Value {
    readable(&manifest(), owner, &board(secrets))
}

#[test]
fn an_update_from_the_same_github_source_keeps_the_secrets() {
    let installed = Owner::new(
        FIRST,
        &folder("a"),
        Some(&repository("Org/Fleet", "dist", "c1")),
    );
    let updated = Owner::new(
        SECOND,
        &folder("b"),
        Some(&repository("org/fleet", "dist", "c2")),
    );
    assert!(same(&installed, &updated));
    let saved = Map::from_iter([(installed.stored_key("fleet-a"), json!("t"))]);
    assert_eq!(reads(&updated, &saved), json!({"fleet-a": "t"}));
}

#[test]
fn another_repository_or_board_folder_does_not_see_them() {
    let installed = Owner::new(
        FIRST,
        &folder("a"),
        Some(&repository("org/fleet", "dist", "c1")),
    );
    let saved = Map::from_iter([(installed.stored_key("fleet-a"), json!("t"))]);
    for other in [
        repository("org/other", "dist", "c1"),
        repository("org/fleet", "web", "c1"),
    ] {
        let other = Owner::new(FIRST, &folder("a"), Some(&other));
        assert_eq!(reads(&other, &saved), json!({}));
    }
}

#[test]
fn local_folders_releases_and_websites_are_owned_by_their_source() {
    let copied = |original: &str| ManagedSource::Folder {
        original: original.into(),
        content: "c".into(),
    };
    let from = |identity, name, managed| Owner::new(identity, &folder(name), Some(&managed));
    assert!(same(
        &from(FIRST, "a", copied("/apps/fleet")),
        &from(SECOND, "b", copied("/apps/fleet"))
    ));
    assert!(!same(
        &from(FIRST, "a", copied("/apps/fleet")),
        &from(FIRST, "a", copied("/apps/other"))
    ));
    let release = |tag: &str, asset: &str| ManagedSource::Release {
        repository: "org/fleet".into(),
        release_id: 1,
        tag: tag.into(),
        asset_name: asset.into(),
        include_prereleases: false,
        content: tag.into(),
    };
    assert!(same(
        &from(FIRST, "a", release("v1", "fleet.zip")),
        &from(SECOND, "b", release("v2", "fleet.zip"))
    ));
    assert!(!same(
        &from(FIRST, "a", release("v1", "fleet.zip")),
        &from(FIRST, "a", release("v1", "other.zip"))
    ));
    let site = |url: &str| {
        Owner::new(
            FIRST,
            &WebBoardSource::Url(url.into()),
            Some(&ManagedSource::Website),
        )
    };
    assert!(same(
        &site("https://a.example/one"),
        &site("https://a.example/two")
    ));
    assert!(!same(
        &site("https://a.example/"),
        &site("https://b.example/")
    ));
}

#[test]
fn a_zip_reinstall_and_an_unrecorded_source_stay_strict() {
    let first = Owner::new(FIRST, &folder("a"), Some(&zip("c1")));
    let again = Owner::new(SECOND, &folder("b"), Some(&zip("c1")));
    assert!(!same(&first, &again));
    let saved = Map::from_iter([
        (first.stored_key("fleet-a"), json!("t")),
        ("fleet-b".to_owned(), json!("ownerless")),
    ]);
    assert_eq!(reads(&first, &saved), json!({"fleet-a": "t"}));
    assert_eq!(reads(&again, &saved), json!({}));
    // No record, or one that does not match how the Space is shown.
    assert_eq!(Owner::new(FIRST, &folder("a"), None), first);
    let mismatched = Owner::new(FIRST, &folder("a"), Some(&ManagedSource::Website));
    assert_eq!(mismatched, first);
    assert_eq!(first.forms("fleet-a"), [first.stored_key("fleet-a")]);
}

#[test]
fn earlier_keys_are_read_as_this_apps_and_replaced_on_save() {
    let owner = Owner::new(
        FIRST,
        &folder("a"),
        Some(&repository("org/fleet", "dist", "c1")),
    );
    let identity_key = format!("{}:fleet-b", &FIRST[..32]);
    let saved = Map::from_iter([
        (owner.stored_key("fleet-a"), json!("current")),
        ("fleet-a".to_owned(), json!("older")),
        (identity_key.clone(), json!("folder and manifest")),
        ("fleet-c".to_owned(), json!("ownerless")),
        ("token".to_owned(), json!("declared")),
    ]);
    assert_eq!(
        reads(&owner, &saved),
        json!({"fleet-a": "current", "fleet-b": "folder and manifest",
            "fleet-c": "ownerless", "token": "declared"})
    );
    assert_eq!(
        owner.forms("fleet-b"),
        [
            owner.stored_key("fleet-b"),
            identity_key,
            "fleet-b".to_owned()
        ]
    );
    // Without a recorded source, ownerless keys stay hidden.
    let strict = Owner::new(FIRST, &folder("a"), None);
    assert_eq!(
        reads(&strict, &saved),
        json!({"fleet-b": "folder and manifest", "token": "declared"})
    );
}

fn entry(managed: Option<&ManagedSource>) -> Map<String, Value> {
    let mut entry = json!({"path": "/store/a", "secrets": {
        "fleet-a": "ownerless", "token": "declared",
        (format!("{}:fleet-b", &FIRST[..32])): "folder and manifest",
    }})
    .as_object()
    .unwrap()
    .clone();
    if let Some(managed) = managed {
        entry.insert(
            "managedSource".into(),
            serde_json::to_value(managed).unwrap(),
        );
    }
    entry
}

#[test]
fn installing_binds_earlier_keys_to_the_source_they_were_saved_under() {
    let installed = repository("org/fleet", "dist", "c1");
    let mut bound = entry(Some(&installed));
    assert!(recorded(&bound) == Some(installed.clone()));
    bind_unowned(&mut bound, &["token".to_owned()], Some(FIRST));
    let secrets: Map<String, Value> = bound["secrets"].as_object().unwrap().clone();
    assert_eq!(
        secrets.len(),
        3,
        "every secret is kept, so Settings counts them"
    );
    assert_eq!(secrets["token"], "declared");

    // The update from the same repository reads them; another one does not.
    let update = Owner::new(
        SECOND,
        &folder("b"),
        Some(&repository("org/fleet", "dist", "c2")),
    );
    assert_eq!(
        reads(&update, &secrets),
        json!({"fleet-a": "ownerless", "fleet-b": "folder and manifest", "token": "declared"})
    );
    let other = Owner::new(
        SECOND,
        &folder("b"),
        Some(&repository("org/other", "dist", "c2")),
    );
    assert_eq!(reads(&other, &secrets), json!({"token": "declared"}));
}

#[test]
fn installing_over_an_unrecorded_source_hides_ownerless_keys() {
    let mut bound = entry(None);
    bind_unowned(&mut bound, &[], Some(FIRST));
    let secrets: Map<String, Value> = bound["secrets"].as_object().unwrap().clone();
    assert_eq!(secrets.len(), 3);
    assert!(!secrets.contains_key("fleet-a"));
    assert!(
        !secrets.contains_key("token"),
        "the new app does not declare it"
    );
    let next = Owner::new(
        SECOND,
        &folder("b"),
        Some(&repository("org/fleet", "dist", "c2")),
    );
    assert_eq!(reads(&next, &secrets), json!({}));
    // The earlier folder-and-manifest key stays strict to its own app.
    let same = Owner::new(FIRST, &folder("a"), None);
    assert_eq!(
        reads(&same, &secrets),
        json!({"fleet-b": "folder and manifest"})
    );
}
