use std::path::Path;

use serde_json::{Value, json};
use subspace_framework::core::config::web_boards::WebBoardSource;

use super::config_file::save_unless_changed;
use super::files::swap_in;
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

/// The grant key of `permission` for the manifest at `root`.
fn key(root: &Path, permission: &str) -> String {
    let manifest = crate::manifest::read(root).unwrap().unwrap();
    format!("{}.{permission}", manifest.identity(root))
}

/// The probe entry's grants, sorted.
fn grants(config: &Path) -> Vec<(String, bool)> {
    let mut grants: Vec<(String, bool)> = saved(config)["webBoards"]["probe"]["permissions"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, on)| (key.clone(), on.as_bool().unwrap()))
        .collect();
    grants.sort();
    grants
}

/// Denies the probe entry's `permission` grants, as Settings saves it.
fn deny(config: &Path, permission: &str) {
    let mut value = saved(config);
    for (key, on) in value["webBoards"]["probe"]["permissions"]
        .as_object_mut()
        .unwrap()
    {
        if key.ends_with(&format!(".{permission}")) {
            *on = Value::Bool(false);
        }
    }
    write(config, value);
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
        assert!(!store.join("probe").exists(), "{entry}");
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
    deny(&config, "secrets");
    let new = home.path().join("new");
    assert!(install(&PROBE, &config, &new).unwrap());
    let path = saved(&config)["webBoards"]["probe"]["path"].clone();
    assert_eq!(path, new.join("probe").to_string_lossy().as_ref());
    // The grants move to the new folder's identity, the denial with them.
    assert_eq!(
        grants(&config),
        [
            (key(&new.join("probe"), "secrets"), false),
            (key(&new.join("probe"), "settings"), true)
        ]
    );

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

#[test]
fn a_changed_manifest_keeps_each_grant_by_name_and_grants_new_ones() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    let store = home.path().join("store");
    install(&PROBE, &config, &store).unwrap();
    deny(&config, "secrets");
    let updated = BuiltIn {
        files: &[
            ("index.html", b"<p>probe</p>"),
            (
                "subspace-board.json",
                br#"{"name": "Probe", "version": "1.1.0", "permissions": ["settings", "secrets", "badge"]}"#,
            ),
            ("src/main.js", b"export {};"),
        ],
        ..PROBE
    };
    assert!(install(&updated, &config, &store).unwrap());
    let root = store.join("probe");
    assert_eq!(
        grants(&config),
        [
            (key(&root, "badge"), true),
            (key(&root, "secrets"), false),
            (key(&root, "settings"), true)
        ]
    );
}

#[test]
fn a_users_folder_space_at_the_built_in_folder_keeps_its_files() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    let store = home.path().join("store");
    let root = store.join("probe");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("index.html"), "mine").unwrap();
    let text = json!({"webBoards": {"probe": {"path": root}}}).to_string();
    std::fs::write(&config, &text).unwrap();
    assert!(!install(&PROBE, &config, &store).unwrap());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), text);
    assert_eq!(std::fs::read(root.join("index.html")).unwrap(), b"mine");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
}

#[test]
fn a_config_changed_since_it_was_read_is_not_saved() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    std::fs::write(&config, "{\"repos\": []}").unwrap();
    assert!(!save_unless_changed(&config, Some("{}"), "{\"webBoards\": {}}").unwrap());
    assert!(!save_unless_changed(&config, None, "{}").unwrap());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "{\"repos\": []}");
    assert!(save_unless_changed(&config, Some("{\"repos\": []}"), "{}").unwrap());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "{}");
}

#[test]
fn a_failed_swap_puts_the_previous_files_back() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("probe");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("index.html"), "previous").unwrap();
    let missing = home.path().join("missing");
    let aside = home.path().join("aside");
    assert!(swap_in(&missing, &root, &aside).is_err());
    assert_eq!(std::fs::read(root.join("index.html")).unwrap(), b"previous");
    assert!(!aside.exists());
}

#[cfg(unix)]
#[test]
fn a_link_in_place_of_the_folder_is_replaced_not_followed() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config.json");
    let store = home.path().join("store");
    let elsewhere = home.path().join("elsewhere");
    std::fs::create_dir_all(elsewhere.join("src")).unwrap();
    for (relative, bytes) in PROBE.files {
        std::fs::write(elsewhere.join(relative), bytes).unwrap();
    }
    std::fs::create_dir_all(&store).unwrap();
    std::os::unix::fs::symlink(&elsewhere, store.join("probe")).unwrap();
    assert!(install(&PROBE, &config, &store).unwrap());
    let root = store.join("probe");
    assert!(std::fs::symlink_metadata(&root).unwrap().is_dir());
    assert_eq!(
        std::fs::read(root.join("index.html")).unwrap(),
        b"<p>probe</p>"
    );
    // The link's target is untouched, and only the folder is in the store.
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 3);
    assert_eq!(std::fs::read_dir(&store).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn a_linked_config_file_is_saved_through_the_link() {
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join("dotfiles").join("config.json");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    write(&target, json!({"repos": ["acme/web"]}));
    let config = home.path().join("config.json");
    std::os::unix::fs::symlink(&target, &config).unwrap();
    assert!(install(&PROBE, &config, &home.path().join("store")).unwrap());
    assert!(
        std::fs::symlink_metadata(&config)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let saved = saved(&target);
    assert_eq!(saved["repos"], json!(["acme/web"]));
    assert_eq!(saved["webBoards"]["probe"]["builtIn"], true);
}
