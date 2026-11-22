use std::path::Path;

use serde_json::{Value, json};
use subspace_framework::core::config::web_boards::WebBoardSource;

use super::{BuiltIn, install};

const MANIFEST: &[u8] =
    br#"{"name": "Probe", "version": "1.0.0", "permissions": ["settings", "secrets"]}"#;

const PROBE: BuiltIn = BuiltIn {
    id: "probe",
    title: "Probe",
    files: &[
        ("index.html", b"<p>probe</p>"),
        ("subspace-board.json", MANIFEST),
        ("src/main.js", b"export {};"),
    ],
};

fn saved(file: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

fn write(file: &Path, value: Value) {
    std::fs::write(file, value.to_string()).unwrap();
}

#[test]
fn a_fresh_config_gets_an_entry_the_app_loads_with_its_grants() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    let store = home.path().join("store");
    assert!(install(&PROBE, &config, &store).unwrap());
    let root = store.join("probe");
    assert_eq!(
        std::fs::read(root.join("src/main.js")).unwrap(),
        b"export {};"
    );
    let entry = &saved(&config)["webBoards"]["probe"];
    assert_eq!(entry["title"], "Probe");
    assert_eq!(entry["builtIn"], true);
    assert_eq!(entry["permissions"].as_object().unwrap().len(), 2);
    assert!(
        entry["permissions"]
            .as_object()
            .unwrap()
            .values()
            .all(|on| on == true)
    );
    // The app's own parser accepts it as a folder board.
    let (parsed, notices) = subspace_framework::core::config::load(&config);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(parsed.web_boards[0].source, WebBoardSource::Folder(root));
    // Nothing changes on the next start.
    let before = std::fs::read(&config).unwrap();
    assert!(!install(&PROBE, &config, &store).unwrap());
    assert_eq!(std::fs::read(&config).unwrap(), before);
}

#[test]
fn an_entry_without_a_source_is_adopted_with_its_settings_and_secrets() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    write(
        &config,
        json!({"repos": ["acme/web"], "webBoards": {"probe": {
            "settings": {"endpoint": "http://127.0.0.1:8484"},
            "secrets": {"token": "fixture-only"},
            "permissions": {"kept": false}
        }}}),
    );
    assert!(install(&PROBE, &config, &home.path().join("store")).unwrap());
    let saved = saved(&config);
    let entry = &saved["webBoards"]["probe"];
    assert_eq!(entry["settings"]["endpoint"], "http://127.0.0.1:8484");
    assert_eq!(entry["secrets"]["token"], "fixture-only");
    assert_eq!(entry["permissions"]["kept"], false);
    assert_eq!(entry["permissions"].as_object().unwrap().len(), 3);
    assert_eq!(saved["repos"], json!(["acme/web"]));
}

#[test]
fn the_users_own_entries_and_choices_are_left_alone() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    let store = home.path().join("store");
    for entry in [
        json!({"url": "https://probe.example.com"}),
        json!({"path": "/apps/probe"}),
        json!({"path": "/apps/probe", "builtIn": true, "managedSource": {"kind": "website"}}),
        json!("not an object"),
    ] {
        let text = json!({"webBoards": {"probe": entry}}).to_string();
        std::fs::write(&config, &text).unwrap();
        assert!(!install(&PROBE, &config, &store).unwrap());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), text);
    }
    // A revoked grant stays revoked.
    write(&config, json!({}));
    install(&PROBE, &config, &store).unwrap();
    let mut value = saved(&config);
    for on in value["webBoards"]["probe"]["permissions"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        *on = Value::Bool(false);
    }
    write(&config, value.clone());
    assert!(!install(&PROBE, &config, &store).unwrap());
    assert_eq!(saved(&config), value);
}

#[test]
fn a_moved_store_updates_the_path_and_changed_files_are_replaced() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    let old = home.path().join("old");
    install(&PROBE, &config, &old).unwrap();
    let new = home.path().join("new");
    assert!(install(&PROBE, &config, &new).unwrap());
    let path = saved(&config)["webBoards"]["probe"]["path"].clone();
    assert_eq!(path, new.join("probe").to_string_lossy().as_ref());

    let root = new.join("probe");
    std::fs::write(root.join("index.html"), "edited").unwrap();
    std::fs::write(root.join("stale.js"), "left over").unwrap();
    assert!(!install(&PROBE, &config, &new).unwrap());
    assert_eq!(
        std::fs::read(root.join("index.html")).unwrap(),
        b"<p>probe</p>"
    );
    assert!(!root.join("stale.js").exists());
    // Only the Space's folder is left in the store.
    assert_eq!(std::fs::read_dir(&new).unwrap().count(), 1);
}

#[test]
fn an_unreadable_config_is_not_overwritten() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    std::fs::write(&config, "{ not json").unwrap();
    assert!(install(&PROBE, &config, &home.path().join("store")).is_err());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "{ not json");
    let unsafe_name = BuiltIn {
        files: &[("../escape.html", b"x")],
        ..PROBE
    };
    assert!(install(&unsafe_name, &config, &home.path().join("other")).is_err());
    assert!(!home.path().join("escape.html").exists());
}
