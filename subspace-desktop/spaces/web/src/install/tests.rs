use std::io::Read;
use std::{io::Write, path::Path};

use super::{ImportRequest, prepare};

mod github;

fn board(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("index.html"), "<h1>Test board</h1>").unwrap();
    std::fs::write(
        root.join("subspace-board.json"),
        r#"{"name":"Test board","version":"1"}"#,
    )
    .unwrap();
}

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn folder_is_a_private_snapshot_and_drop_rolls_back_only_new_files() {
    let source = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    board(source.path());
    let prepared = prepare(
        ImportRequest::Folder {
            path: source.path().into(),
        },
        store.path(),
        None,
    )
    .unwrap();
    let subspace_framework::core::config::web_boards::WebBoardSource::Folder(root) =
        prepared.source()
    else {
        panic!("folder");
    };
    assert_ne!(root, source.path());
    std::fs::write(source.path().join("index.html"), "changed").unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("index.html")).unwrap(),
        "<h1>Test board</h1>"
    );
    drop(prepared);
    assert_eq!(std::fs::read_dir(store.path()).unwrap().count(), 0);
    assert!(source.path().join("index.html").is_file());
}

#[test]
fn new_imports_require_manifest_and_index_but_keep_originals() {
    let source = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("index.html"), "test").unwrap();
    assert!(
        prepare(
            ImportRequest::Folder {
                path: source.path().into()
            },
            store.path(),
            None
        )
        .is_err()
    );
    assert!(source.path().join("index.html").exists());
    assert_eq!(std::fs::read_dir(store.path()).unwrap().count(), 0);
}

#[test]
fn unsafe_zip_paths_collisions_and_special_names_fail() {
    for name in [
        "../escape",
        "/absolute",
        "C:/drive",
        "a\\b",
        "CON",
        "a.",
        ".git/config",
        "a:stream",
        "a//b",
        "a/./b",
        "unicodé",
    ] {
        let bytes = archive(&[(name, b"test")]);
        let root = tempfile::tempdir().unwrap();
        assert!(
            super::archive::extract(&bytes, root.path()).is_err(),
            "{name}"
        );
    }
    let bytes = archive(&[("A.txt", b"one"), ("a.txt", b"two")]);
    let root = tempfile::tempdir().unwrap();
    assert!(super::archive::extract(&bytes, root.path()).is_err());
}

#[test]
fn zip_accepts_one_wrapper_and_rejects_ambiguous_roots() {
    let manifest = br#"{"name":"Test","version":"1"}"#;
    let bytes = archive(&[
        ("package/index.html", b"test"),
        ("package/subspace-board.json", manifest),
    ]);
    let store = tempfile::tempdir().unwrap();
    let zip = store.path().join("source.zip");
    std::fs::write(&zip, bytes).unwrap();
    let prepared = prepare(ImportRequest::Zip { path: zip.clone() }, store.path(), None).unwrap();
    assert_eq!(prepared.name(), Some("Test"));
    prepared.retain();
    assert!(zip.exists());
    let bytes = archive(&[("one/index.html", b"test"), ("two/index.html", b"test")]);
    std::fs::write(&zip, bytes).unwrap();
    assert!(prepare(ImportRequest::Zip { path: zip }, store.path(), None).is_err());
}

#[cfg(unix)]
#[test]
fn folder_links_and_hard_links_are_not_copied() {
    let source = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    board(source.path());
    std::os::unix::fs::symlink("index.html", source.path().join("link")).unwrap();
    assert!(
        prepare(
            ImportRequest::Folder {
                path: source.path().into()
            },
            store.path(),
            None
        )
        .is_err()
    );
    std::fs::remove_file(source.path().join("link")).unwrap();
    std::fs::hard_link(source.path().join("index.html"), source.path().join("hard")).unwrap();
    assert!(
        prepare(
            ImportRequest::Folder {
                path: source.path().into()
            },
            store.path(),
            None
        )
        .is_err()
    );
}

#[test]
fn website_policy_rejects_embedded_credentials_and_non_loopback_http() {
    let store = tempfile::tempdir().unwrap();
    for url in [
        "https://dummy:fake@example.test",
        "http://example.test",
        "file:///test",
    ] {
        assert!(
            prepare(
                ImportRequest::Website { url: url.into() },
                store.path(),
                None
            )
            .is_err()
        );
    }
    assert!(
        prepare(
            ImportRequest::Website {
                url: "http://localhost:1234".into()
            },
            store.path(),
            None
        )
        .is_ok()
    );
}

#[test]
fn zip_links_duplicate_names_and_declared_index_exhaustion_are_rejected() {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.add_symlink(
        "link",
        "../outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let root = tempfile::tempdir().unwrap();
    assert!(super::archive::extract(&bytes, root.path()).is_err());

    let mut bytes = archive(&[("a.txt", b"one"), ("b.txt", b"two")]);
    for offset in 0..bytes.len() - 5 {
        if &bytes[offset..offset + 5] == b"b.txt" {
            bytes[offset] = b'a';
        }
    }
    assert!(super::archive::extract(&bytes, root.path()).is_err());
    let mut bytes = archive(&[("test", b"one")]);
    let end = bytes.len() - 22;
    bytes[end + 8..end + 12].fill(255);
    assert!(super::archive::extract(&bytes, root.path()).is_err());
}

#[test]
fn limits_count_bytes_while_streaming_not_only_archive_metadata() {
    let mut budget = super::files::Budget::default();
    let mut excessive = std::io::repeat(0).take(super::files::MAX_FILE_BYTES + 1);
    assert!(budget.copy(&mut excessive, &mut std::io::sink()).is_err());
    let mut budget = super::files::Budget::default();
    for _ in 0..8 {
        let mut full = std::io::repeat(0).take(super::files::MAX_FILE_BYTES);
        assert!(budget.copy(&mut full, &mut std::io::sink()).is_ok());
    }
    assert!(
        budget
            .copy(&mut &b"one more byte"[..], &mut std::io::sink())
            .is_err()
    );
}

#[test]
fn prepared_summary_and_provenance_are_native_and_never_grant_permissions() {
    let source = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    board(source.path());
    std::fs::write(
        source.path().join("subspace-board.json"),
        r#"{"name":"Test","version":"2","permissions":["settings","unknown-test-permission"]}"#,
    )
    .unwrap();
    let prepared = prepare(
        ImportRequest::Folder {
            path: source.path().into(),
        },
        store.path(),
        None,
    )
    .unwrap();
    assert_eq!(prepared.version(), Some("2"));
    assert_eq!(prepared.permissions(), ["settings"]);
    assert_eq!(prepared.warnings().len(), 1);
    assert_eq!(prepared.metadata().label(), "Imported folder");
    assert_eq!(prepared.metadata().revision(), None);
    assert_eq!(prepared.metadata().repository(), None);
    let value = serde_json::to_value(prepared.metadata()).unwrap();
    assert_eq!(value["kind"], "folder");
    assert!(value.get("permissions").is_none());
    assert!(value.get("credential").is_none());
}

#[test]
fn failed_replacement_does_not_touch_retained_assets_or_source_files() {
    let source = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    board(source.path());
    let prepared = prepare(
        ImportRequest::Folder {
            path: source.path().into(),
        },
        store.path(),
        None,
    )
    .unwrap();
    let subspace_framework::core::config::web_boards::WebBoardSource::Folder(old) =
        prepared.source()
    else {
        panic!("folder");
    };
    prepared.retain();
    std::fs::write(source.path().join("subspace-board.json"), "invalid JSON").unwrap();
    assert!(
        prepare(
            ImportRequest::Folder {
                path: source.path().into()
            },
            store.path(),
            None
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(old.join("index.html")).unwrap(),
        "<h1>Test board</h1>"
    );
    assert_eq!(std::fs::read_dir(store.path()).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_to_string(source.path().join("subspace-board.json")).unwrap(),
        "invalid JSON"
    );
}
