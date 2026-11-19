use super::tests::fixture;
use super::{ProjectStore, limits};
use std::collections::BTreeMap;

fn source(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([("main.py".into(), bytes.to_vec())])
}

#[test]
fn atomic_write_creates_and_replaces_file_contents() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.py");
    super::disk::atomic_write(&path, b"first").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"first");
    super::disk::atomic_write(&path, b"replacement").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
}

#[test]
fn concurrent_expected_revision_writes_accept_exactly_one_update() {
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let initial = store.create(&fixture(), source(b"initial")).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let writers: Vec<_> = [b"first".as_slice(), b"second".as_slice()]
        .into_iter()
        .map(|bytes| {
            let store = store.clone();
            let revision = initial.revision.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.edit_file("demo", "main.py", bytes, &revision).is_ok()
            })
        })
        .collect();
    barrier.wait();
    let successes = writers
        .into_iter()
        .map(|writer| usize::from(writer.join().unwrap()))
        .sum::<usize>();
    assert_eq!(successes, 1);
    let current = store.load("demo").unwrap();
    assert!(matches!(
        current.files["main.py"].as_slice(),
        b"first" | b"second"
    ));
    assert_ne!(current.revision, initial.revision);
}

#[test]
fn edits_require_current_source_and_definition_revisions_and_snapshots_do_not_move() {
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let initial = store
        .create(&fixture(), source(b"print('initial')"))
        .unwrap();
    let snapshot = store.snapshot("demo", &initial.revision).unwrap();
    let edited = store
        .edit_file("demo", "main.py", b"print('edited')", &initial.revision)
        .unwrap();
    assert_eq!(edited.revision.definition, initial.revision.definition);
    assert_ne!(edited.revision.source, initial.revision.source);
    assert!(
        store
            .edit_file("demo", "main.py", b"overwrite", &initial.revision)
            .is_err()
    );
    let mut definition = edited.definition.clone();
    definition.title = "Updated".into();
    assert!(store.save(&definition, &initial.revision).is_err());
    let saved = store.save(&definition, &edited.revision).unwrap();
    assert_ne!(saved.revision.definition, edited.revision.definition);
    assert_eq!(store.list().unwrap().len(), 1);
    assert_eq!(store.open("demo").unwrap().definition.title, "Updated");
    let reopened = store
        .open_snapshot("demo", &snapshot.revision().source)
        .unwrap();
    assert_eq!(reopened.files()["main.py"], b"print('initial')");
    assert_eq!(reopened.definition().title, "demo");
    assert_eq!(snapshot.files(), reopened.files());
    assert!(store.create(&fixture(), source(b"duplicate")).is_err());
}

#[test]
fn external_edits_conflict_and_private_or_undeclared_files_never_enter_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let initial = store.create(&fixture(), source(b"initial")).unwrap();
    std::fs::write(initial.directory.join("main.py"), b"external edit").unwrap();
    std::fs::write(
        initial.directory.join(".env"),
        b"DUMMY_TEST_ONLY=not-a-secret",
    )
    .unwrap();
    std::fs::create_dir(root.path().join("private-settings")).unwrap();
    std::fs::write(
        root.path().join("private-settings/test.txt"),
        b"synthetic-private-value",
    )
    .unwrap();
    assert!(store.save(&initial.definition, &initial.revision).is_err());
    assert!(store.snapshot("demo", &initial.revision).is_err());
    let current = store.load("demo").unwrap();
    let snapshot = store.snapshot("demo", &current.revision).unwrap();
    assert_eq!(snapshot.files().len(), 1);
    assert_eq!(snapshot.files()["main.py"], b"external edit");
    assert!(!snapshot.directory().join(".env").exists());
    assert!(
        store
            .edit_file("demo", "extra.py", b"undeclared", &current.revision)
            .is_err()
    );
}

#[test]
fn source_and_definition_byte_limits_are_enforced_without_partial_writes() {
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let loaded = store
        .create(&fixture(), source(&vec![b'x'; limits::FILE_BYTES]))
        .unwrap();
    assert!(
        store
            .edit_file(
                "demo",
                "main.py",
                &vec![b'x'; limits::FILE_BYTES + 1],
                &loaded.revision
            )
            .is_err()
    );
    assert_eq!(store.load("demo").unwrap().revision, loaded.revision);
    let definition_bytes = serde_json::to_vec(&fixture()).unwrap();
    let mut padded = definition_bytes;
    padded.resize(limits::DEFINITION_BYTES, b' ');
    assert!(super::ProjectDefinition::parse(&padded).is_ok());
    padded.push(b' ');
    assert!(super::ProjectDefinition::parse(&padded).is_err());
}

#[test]
fn total_source_and_file_count_budgets_are_enforced_at_boundary() {
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let mut definition = fixture();
    definition.files = (0..8).map(|ix| format!("file{ix}.py")).collect();
    definition.entrypoint = "file0.py".into();
    let files: BTreeMap<_, _> = definition
        .files
        .iter()
        .map(|path| (path.clone(), vec![b'x'; limits::FILE_BYTES]))
        .collect();
    let loaded = store.create(&definition, files.clone()).unwrap();
    assert_eq!(
        loaded.files.values().map(Vec::len).sum::<usize>(),
        limits::SOURCE_BYTES
    );
    definition.id = "excess".into();
    definition.files.push("extra.py".into());
    let mut excess = files;
    excess.insert("extra.py".into(), vec![b'x']);
    assert!(store.create(&definition, excess).is_err());
    definition.files = (0..32).map(|ix| format!("file{ix}.py")).collect();
    assert!(definition.validate().is_ok());
    definition.files.push("file32.py".into());
    assert!(definition.validate().is_err());
}

#[cfg(unix)]
#[test]
fn source_and_project_directory_symlinks_fail_closed() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let loaded = store.create(&fixture(), source(b"safe")).unwrap();
    std::fs::write(outside.path().join("outside.py"), b"outside").unwrap();
    std::fs::remove_file(loaded.directory.join("main.py")).unwrap();
    symlink(
        outside.path().join("outside.py"),
        loaded.directory.join("main.py"),
    )
    .unwrap();
    assert!(store.load("demo").is_err());
    assert!(store.snapshot("demo", &loaded.revision).is_err());
    symlink(outside.path(), root.path().join("projects/escape")).unwrap();
    assert!(store.load("escape").is_err());
    assert!(store.load("../private").is_err());
}

/// A lock held for a moment, as by a child that inherited it while another
/// thread was spawning a process, does not fail the next project write.
#[test]
fn project_lock_waits_for_a_briefly_held_lock() {
    let root = tempfile::tempdir().unwrap();
    let holder = super::disk::lock(root.path()).unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(holder);
    });
    let acquired = super::disk::lock(root.path());
    release.join().unwrap();
    assert!(acquired.is_ok(), "{acquired:?}");
}

#[test]
fn project_lock_still_reports_a_concurrent_writer() {
    let root = tempfile::tempdir().unwrap();
    let _holder = super::disk::lock(root.path()).unwrap();
    let error = super::disk::lock(root.path()).unwrap_err();
    assert!(
        error.to_string().contains("another project write"),
        "{error}"
    );
}
