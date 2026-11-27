//! A choice saved under a Space's former id is read as the Space's own, and
//! undoing it later sticks: once the app saves, the file holds only the
//! Space's id, so the old key cannot bring the choice back.

use std::fs;
use std::path::Path;

use gpui_kit::TestAppContext;
use subspace_core::config;
use subspace_core::former_ids::{Renames, install};
use subspace_core::settings::SettingsFile;
use subspace_core::state::State;
use subspace_dashboard::notifications::{self, Mode};
use subspace_dashboard::{prefs, refresh};

use super::form::switched_board_values;

const SPACE: &str = "example-new";

/// Every test in this process that installs former ids installs these.
pub(crate) fn install_example() {
    install(Renames::from([(
        "example-old".to_owned(),
        SPACE.to_owned(),
    )]));
}

fn text(file: &Path) -> String {
    fs::read_to_string(file).unwrap()
}

#[test]
fn a_space_switched_off_under_its_former_id_can_be_switched_on() {
    install_example();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    fs::write(&file, r#"{"boards":{"example-old":false}}"#).unwrap();
    assert!(!config::load(&file).0.board_enabled(SPACE));

    let mut settings = SettingsFile::open(&file).unwrap();
    let boards = switched_board_values(&settings.values().boards, SPACE, true, true);
    settings.save_board_choices(&boards).unwrap();

    let (reloaded, notices) = config::load(&file);
    assert!(notices.is_empty(), "{notices:?}");
    assert!(reloaded.board_enabled(SPACE));
    assert!(!text(&file).contains("example-old"));
}

#[gpui_kit::test]
fn a_preference_reset_under_the_space_s_id_stays_reset(cx: &mut TestAppContext) {
    install_example();
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    fs::write(&state, r#"{"example-old.refresh-interval-seconds":"600"}"#).unwrap();
    cx.update(|cx| {
        prefs::init_file(Some(state.clone()), cx);
        assert_eq!(refresh::preferences(SPACE, cx).interval_seconds, Some(600));
        refresh::Preferences::default().save(SPACE, cx);
        prefs::save(cx).unwrap();
    });

    let reloaded = State::load(&state);
    assert_eq!(reloaded.get("example-new.refresh-interval-seconds"), None);
    assert!(!text(&state).contains("example-old"));
}

#[test]
fn an_mcp_group_switched_off_under_its_former_id_stays_off() {
    install_example();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    fs::write(&file, r#"{"mcp":{"groups":{"example-old":false}}}"#).unwrap();
    assert!(!config::load(&file).0.mcp.group_enabled(SPACE));

    let mut settings = SettingsFile::open(&file).unwrap();
    let values = settings.values().clone();
    settings.save(&values).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&text(&file)).unwrap();
    assert_eq!(
        saved["mcp"]["groups"],
        serde_json::json!({"example-new": false})
    );
}

#[test]
fn a_config_settings_refuses_still_switches_the_group_off() {
    install_example();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    let original = r#"{"repos":"not a list","mcp":{"groups":{"example-old":false}}}"#;
    fs::write(&file, original).unwrap();
    assert!(SettingsFile::open(&file).is_err());
    let (loaded, notices) = config::load(&file);
    assert!(!notices.is_empty());
    assert!(!loaded.mcp.group_enabled(SPACE));
    assert_eq!(text(&file), original);
}

#[gpui_kit::test]
fn a_space_muted_under_its_former_id_stays_muted(cx: &mut TestAppContext) {
    install_example();
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    fs::write(
        &state,
        r#"{"notifications.example-old.mode":"off","notifications.example-old.chosen":"1"}"#,
    )
    .unwrap();
    cx.update(|cx| {
        prefs::init_file(Some(state.clone()), cx);
        let system = notifications::Preferences {
            mode: Mode::System,
            ..Default::default()
        };
        notifications::install_defaults([(SPACE, system)], cx);
        assert_eq!(notifications::preferences(SPACE, cx).mode, Mode::Off);
        system.save(SPACE, cx);
        prefs::save(cx).unwrap();
    });

    let reloaded = State::load(&state);
    assert_eq!(
        reloaded.get("notifications.example-new.mode"),
        Some("system")
    );
    assert!(!text(&state).contains("example-old"));
}
