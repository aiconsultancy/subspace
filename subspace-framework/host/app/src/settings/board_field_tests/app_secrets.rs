//! Secrets a board's app saves itself: counted, never shown, cleared on Save.

use gpui_kit::{TestAppContext, WindowHandle};
use serde_json::json;
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;

use super::super::Settings;
use super::super::board_fields::app_secrets_text;
use super::fields;
use crate::theme;

const CONFIG: &str = r#"{"webBoards": {"notes": {
    "url": "https://notes.example",
    "secrets": {"token": "declared", "fleet-a": "app-a", "fleet-b": "app-b"}
}}}"#;

fn open(
    root: &std::path::Path,
    app_secrets: bool,
    cx: &mut TestAppContext,
) -> WindowHandle<Settings> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        theme::initialize(cx);
        let mut board = super::super::navigation_tests::registration();
        board.id = "notes";
        let source = move || {
            Some(if app_secrets {
                fields().with_app_secrets()
            } else {
                fields()
            })
        };
        cx.set_global(crate::registry::IncludedBoards(vec![
            board.with_field_source(source),
        ]));
    });
    let path = root.join("config.json");
    std::fs::write(&path, CONFIG).unwrap();
    let file = SettingsFile::open(&path).unwrap();
    cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx))
}

#[gpui_kit::test]
fn app_secrets_are_counted_and_cleared_on_save_leaving_declared_ones(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), true, cx);
    settings
        .update(cx, |settings, window, cx| {
            let app = settings.board_fields[0].app_secrets.as_ref().unwrap();
            assert_eq!(app.keys, ["fleet-a", "fleet-b"]);
            assert!(!super::super::board_fields::changed(
                &settings.board_fields,
                cx
            ));
            settings.toggle_app_secrets_clear(0, cx);
            assert!(super::super::board_fields::changed(
                &settings.board_fields,
                cx
            ));
            settings.toggle_app_secrets_clear(0, cx);
            assert!(!super::super::board_fields::changed(
                &settings.board_fields,
                cx
            ));
            settings.toggle_app_secrets_clear(0, cx);
            assert!(!settings.save(window, cx).unwrap());
            let app = settings.board_fields[0].app_secrets.as_ref().unwrap();
            assert!(app.keys.is_empty() && !app.clear);
        })
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(
        saved["webBoards"]["notes"]["secrets"],
        json!({"token": "declared"})
    );
}

#[gpui_kit::test]
fn a_board_whose_app_saves_no_secrets_shows_no_count(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), false, cx);
    settings
        .update(cx, |settings, _, _| {
            assert!(settings.board_fields[0].app_secrets.is_none());
        })
        .unwrap();
    assert_eq!(app_secrets_text(1), "This app has saved 1 secret");
    assert_eq!(app_secrets_text(3), "This app has saved 3 secrets");
}
