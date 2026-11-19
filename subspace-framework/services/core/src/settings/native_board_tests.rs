use std::{collections::BTreeMap, fs};

use serde_json::{Map, Value, json};

use super::{SettingsError, SettingsFile};
use crate::config;

fn changes(key: &str, value: Value) -> BTreeMap<String, Map<String, Value>> {
    BTreeMap::from([(
        "fixture".to_owned(),
        Map::from_iter([(key.to_owned(), value)]),
    )])
}

fn json_file(path: &std::path::Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn native_board_drafts_save_to_the_boards_own_file_and_cancel_does_not_write() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    fs::write(&path, r#"{"extension":7}"#).unwrap();
    let board = config::board_file(&path, "fixture");
    let mut file = SettingsFile::open(&path).unwrap();
    assert!(file.open_board_file("fixture").unwrap().is_empty());
    let mut draft = file.values().clone();
    let apps = json!([
        {"id":"second","name":"Edited","program":"/test/editor","args":["a b"],"applyEnvironment":false},
        {"id":"first","name":"First","program":"/test/first","args":[],"applyEnvironment":true}
    ]);
    draft.board_files = changes("apps", apps.clone());
    assert!(!board.exists());
    file.save(&draft).unwrap();

    assert_eq!(json_file(&board), json!({"apps": apps}));
    let saved = json_file(&path);
    assert_eq!(saved["extension"], 7);
    assert!(
        !saved.to_string().contains("applyEnvironment"),
        "board settings stay out of the config file"
    );
    assert_eq!(file.board_file_values("fixture")["apps"], apps);
    let (parsed, notices) = config::load_with_boards(&path, ["fixture"]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(parsed.board_settings["fixture"]["apps"], apps);
    assert!(config::load(&path).0.board_settings.is_empty());

    let before = fs::read(&board).unwrap();
    draft.board_files = changes("apps", json!([]));
    drop(file);
    assert_eq!(fs::read(&board).unwrap(), before);
}

#[test]
fn a_save_keeps_the_board_files_other_keys() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    let board = config::board_file(&path, "fixture");
    fs::create_dir_all(board.parent().unwrap()).unwrap();
    fs::write(&board, r#"{"keep":{"nested":7},"apps":[1]}"#).unwrap();
    let mut file = SettingsFile::open(&path).unwrap();
    assert_eq!(file.open_board_file("fixture").unwrap()["apps"], json!([1]));
    let mut draft = file.values().clone();
    draft.board_files = changes("apps", json!([2]));
    file.save(&draft).unwrap();
    assert_eq!(
        json_file(&board),
        json!({"keep": {"nested": 7}, "apps": [2]})
    );
}

#[test]
fn a_board_file_changed_elsewhere_refuses_the_save_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    fs::write(&path, "{}\n").unwrap();
    let board = config::board_file(&path, "fixture");
    let mut file = SettingsFile::open(&path).unwrap();
    file.open_board_file("fixture").unwrap();
    fs::create_dir_all(board.parent().unwrap()).unwrap();
    fs::write(&board, r#"{"external":true}"#).unwrap();
    let mut draft = file.values().clone();
    draft.auto_refresh = false;
    draft.board_files = changes("apps", json!([]));
    let error = file.save(&draft).unwrap_err();
    assert!(
        matches!(&error, SettingsError::BoardFile { error, .. } if matches!(**error, SettingsError::Stale))
    );
    assert!(error.to_string().starts_with(&board.display().to_string()));
    assert_eq!(fs::read_to_string(&path).unwrap(), "{}\n");
    assert_eq!(json_file(&board), json!({"external": true}));
}

#[test]
fn an_invalid_board_file_is_named_when_opened() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    let board = config::board_file(&path, "fixture");
    fs::create_dir_all(board.parent().unwrap()).unwrap();
    fs::write(&board, "{ nope").unwrap();
    let mut file = SettingsFile::open(&path).unwrap();
    let error = file.open_board_file("fixture").unwrap_err();
    assert!(error.to_string().starts_with(&board.display().to_string()));
    fs::write(&board, "[]").unwrap();
    assert!(matches!(
        file.open_board_file("fixture"),
        Err(SettingsError::BoardFile { .. })
    ));
}

#[cfg(unix)]
#[test]
fn board_files_and_their_folders_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    let board = config::board_file(&path, "fixture");
    fs::create_dir_all(board.parent().unwrap()).unwrap();
    fs::set_permissions(board.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&board, "{}").unwrap();
    fs::set_permissions(&board, fs::Permissions::from_mode(0o644)).unwrap();
    let mut file = SettingsFile::open(&path).unwrap();
    file.open_board_file("fixture").unwrap();
    let mut draft = file.values().clone();
    draft.board_files = changes("password", json!("secret"));
    file.save(&draft).unwrap();
    let mode = |path: &std::path::Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&board), 0o600);
    assert_eq!(mode(board.parent().unwrap()), 0o700);
    let (config, notices) = config::load_with_boards(&path, ["fixture"]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(config.board_settings["fixture"]["password"], "secret");
}
