//! The readers that map former ids, on files that they never change.

use std::fs;
use std::path::Path;

use serde_json::json;

use super::super::board_file;
use super::example;
use crate::settings::SettingsFile;
use crate::state::State;

fn write(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, text).unwrap();
    file
}

#[test]
fn state_loads_former_keys_as_the_space_s_own_and_leaves_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let text = r#"{"example-old.layout":{"n":18446744073709551617},
        "example-old.size":"4","example-new.size":{"kept":1.000000000000000000001},
        "example-old.width":3,"example-new.width":"9"}"#;
    let file = write(dir.path(), "state.json", text);
    let state = State::load_with(&file, &example());
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    assert!(state.has_invalid_value("example-new.layout"));
    assert!(state.has_invalid_value("example-new.size"));
    assert_eq!(state.get("example-new.width"), Some("9"));
    assert!(!state.has_invalid_value("example-old.layout"));

    state.save(&file).unwrap();
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.contains(r#"{"n":18446744073709551617}"#), "{saved}");
    assert!(
        saved.contains(r#"{"kept":1.000000000000000000001}"#),
        "{saved}"
    );
    assert!(!saved.contains("example-old"), "{saved}");
}

#[test]
fn the_config_is_parsed_under_the_space_s_id_despite_unrelated_warnings() {
    let text = r#"{"boards":{"example-old":false},"repos":"not a list",
        "mcp":{"groups":{"example-old":false}}}"#;
    let (config, notices) = crate::config::parse_with(text, Path::new("config.json"), &example());
    assert!(!notices.is_empty());
    assert!(!config.board_enabled("example-new"));
    assert!(!config.mcp.group_enabled("example-new"));
    assert!(config.mcp.group_enabled("example-old"));
}

#[test]
fn nothing_is_mapped_before_former_ids_are_installed() {
    let text = r#"{"boards":{"example-old":false}}"#;
    let (config, _) = crate::config::parse(text, Path::new("config.json"));
    assert!(config.board_enabled("example-new"));
    assert!(!config.board_enabled("example-old"));
}

#[test]
fn settings_open_and_save_under_the_space_s_id_only() {
    let dir = tempfile::tempdir().unwrap();
    let text = r#"{"boards":{"example-old":false},"mcp":{"groups":{"example-old":false}},
        "commands":{"aliases":{"example-old":["example-short"]}}}"#;
    let file = write(dir.path(), "config.json", text);
    let mut settings = SettingsFile::open_with(&file, &example()).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
    assert!(!settings.values().boards["example-new"]);
    assert!(!settings.values().mcp.group_enabled("example-new"));

    let values = settings.values().clone();
    settings.save(&values).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved["boards"], json!({"example-new": false}));
    assert_eq!(saved["mcp"]["groups"], json!({"example-new": false}));
    assert_eq!(
        saved["commands"],
        json!({"aliases": {"example-new": ["example-short"]}})
    );
}

#[test]
fn a_space_s_settings_file_is_its_own_then_a_former_one_then_its_own_path() {
    let dir = tempfile::tempdir().unwrap();
    let own = dir.path().join("example-new").join("config.json");
    assert_eq!(board_file(dir.path(), "example-new", &example()), own);

    let former = write(dir.path(), "example-old/config.json", "{}");
    assert_eq!(board_file(dir.path(), "example-new", &example()), former);

    write(dir.path(), "example-new/config.json", "{}");
    assert_eq!(board_file(dir.path(), "example-new", &example()), own);
    assert_eq!(board_file(dir.path(), "example-old", &example()), former);
}

/// A folder that cannot be searched is not a missing file: the loader and
/// the editor must report it, not read a former id's settings in its place.
#[cfg(unix)]
#[test]
fn a_space_s_own_file_that_cannot_be_looked_up_is_still_its_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "example-old/config.json", "{}");
    let own = write(dir.path(), "example-new/config.json", "{}");
    let folder = dir.path().join("example-new");
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o000)).unwrap();
    let resolved = board_file(dir.path(), "example-new", &example());
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(resolved, own);
}

#[test]
fn a_space_s_file_that_appears_while_settings_is_open_makes_the_save_stale() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "config.json", "{}");
    let former = write(dir.path(), "example-old/config.json", r#"{"kept":1}"#);
    let mut settings = SettingsFile::open_with(&file, &example()).unwrap();
    assert_eq!(settings.open_board_file("example-new").unwrap()["kept"], 1);

    let own = write(dir.path(), "example-new/config.json", "{}");
    let mut values = settings.values().clone();
    values.board_files.insert(
        "example-new".into(),
        json!({"kept": 2}).as_object().cloned().unwrap(),
    );
    let error = settings.save(&values).unwrap_err();
    assert!(
        matches!(&error, crate::settings::SettingsError::BoardFile { error, .. }
            if matches!(**error, crate::settings::SettingsError::Stale)),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&former).unwrap(), r#"{"kept":1}"#);
    assert_eq!(fs::read_to_string(&own).unwrap(), "{}");
    assert_eq!(fs::read_to_string(&file).unwrap(), "{}");
}
