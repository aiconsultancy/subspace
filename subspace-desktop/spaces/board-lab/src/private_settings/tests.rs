use super::{Edit, PrivateSettings};
use std::collections::BTreeMap;
use std::path::Path;
use subspace_framework::core::Secret;

#[path = "tests/concurrency.rs"]
mod concurrency;
#[path = "tests/import.rs"]
mod import;
#[path = "tests/limits.rs"]
mod limits;
#[path = "tests/permissions.rs"]
mod permissions;

fn store(root: &Path, id: &str) -> anyhow::Result<PrivateSettings> {
    PrivateSettings::new(root, id)
}

fn replace(name: &str, value: &str) -> BTreeMap<String, Edit> {
    BTreeMap::from([(name.to_owned(), Edit::Replace(Secret::new(value)))])
}

#[test]
fn project_isolation_and_stale_writes() {
    let root = tempfile::tempdir().unwrap();
    let a = store(root.path(), "project-a").unwrap();
    let b = store(root.path(), "project-b").unwrap();
    let initial = a.metadata().unwrap();
    let saved = a
        .update(initial.revision(), replace("API_KEY", "fake-a"))
        .unwrap();
    assert_eq!(saved.names(), ["API_KEY"]);
    assert!(
        a.update(initial.revision(), replace("API_KEY", "fake-overwrite"))
            .is_err()
    );
    assert_eq!(
        a.selected(&["API_KEY"]).unwrap().into_values()["API_KEY"],
        "fake-a"
    );
    assert!(b.metadata().unwrap().names().is_empty());
    assert!(b.selected(&["API_KEY"]).is_err());
    assert!(PrivateSettings::new(root.path(), "../other").is_err());
}

#[test]
fn keep_replace_clear_and_names_only_debug() {
    let root = tempfile::tempdir().unwrap();
    let settings = store(root.path(), "project").unwrap();
    let revision = settings.metadata().unwrap().revision();
    let saved = settings
        .update(revision, replace("API_KEY", "dummy-private-value"))
        .unwrap();
    let kept = settings
        .update(
            saved.revision(),
            BTreeMap::from([("API_KEY".into(), Edit::Keep)]),
        )
        .unwrap();
    assert_eq!(
        settings.selected(&["API_KEY"]).unwrap().into_values()["API_KEY"],
        "dummy-private-value"
    );
    let selection = settings.selected(&["API_KEY"]).unwrap();
    for output in [
        format!("{kept:?}"),
        format!("{selection:?}"),
        format!("{:?}", Edit::Replace(Secret::new("dummy-private-value"))),
    ] {
        assert!(!output.contains("dummy-private-value"));
    }
    let replaced = settings
        .update(kept.revision(), replace("API_KEY", "other-fake"))
        .unwrap();
    assert_eq!(
        settings.selected(&["API_KEY"]).unwrap().into_values()["API_KEY"],
        "other-fake"
    );
    let cleared = settings
        .update(
            replaced.revision(),
            BTreeMap::from([("API_KEY".into(), Edit::Clear)]),
        )
        .unwrap();
    assert!(cleared.names().is_empty());
    assert!(settings.selected(&["API_KEY"]).is_err());
}
