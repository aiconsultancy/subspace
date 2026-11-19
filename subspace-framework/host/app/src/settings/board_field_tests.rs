mod integration;

use std::path::Path;

use gpui_kit::{TestAppContext, WindowHandle};
use serde_json::json;
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_dashboard::Tokens;
use subspace_dashboard::settings::{BoardFields, FieldKind, FieldSpec, PermissionSpec};

use super::Settings;
use super::board_fields::Control;
use crate::theme;

fn spec(key: &str, kind: FieldKind) -> FieldSpec {
    FieldSpec::new(
        key.into(),
        key.into(),
        Some(format!("Help for {key}")),
        kind,
    )
}

fn fields() -> BoardFields {
    BoardFields::new(
        vec!["webBoards".into(), "notes".into()],
        vec![
            spec("endpoint", FieldKind::Url { default: None }),
            spec(
                "limit",
                FieldKind::Number {
                    default: Some(10.0),
                },
            ),
            spec("compact", FieldKind::Boolean { default: false }),
            spec(
                "theme",
                FieldKind::Select {
                    options: vec!["light".into(), "dark".into()],
                    default: Some("light".into()),
                },
            ),
            spec("token", FieldKind::Secret),
            spec("key", FieldKind::Secret),
        ],
        vec!["Notes 1.0.0".into()],
        vec![PermissionSpec::new(
            "identity.settings".into(),
            "Read settings".into(),
        )],
    )
}

const CONFIG: &str = r#"{"webBoards": {"notes": {
    "url": "https://notes.example",
    "settings": {"limit": 3, "kept": "unknown keys stay"},
    "secrets": {"token": "old-secret"}
}}}"#;

fn open(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        theme::initialize(cx);
        let mut board = super::navigation_tests::registration();
        board.id = "notes";
        cx.set_global(crate::registry::IncludedBoards(vec![
            board.with_field_source(|| Some(fields())),
        ]));
    });
    let path = root.join("config.json");
    std::fs::write(&path, CONFIG).unwrap();
    let file = SettingsFile::open(&path).unwrap();
    cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx))
}

fn input(
    settings: &Settings,
    field: usize,
) -> gpui_kit::Entity<gpui_kit::component::input::InputState> {
    match &settings.board_fields[0].fields[field].control {
        Control::Text { input, .. } | Control::Secret { input, .. } => input.clone(),
        _ => panic!("field {field} has no input"),
    }
}

#[gpui_kit::test]
fn immediate_membership_changes_keep_dirty_entities_and_never_resurrect_removed_fields(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            let retained_repos = settings.form.repos.clone();
            retained_repos.update(cx, |list, cx| list.toggle("owner/unsaved", cx));
            let retained_limit = input(settings, 1);
            retained_limit.update(cx, |input, cx| input.set_value("99", window, cx));
            let path = root.path().join("config.json");
            let mut manager = SettingsFile::open(&path).unwrap();
            add_wiki(&mut manager, cx);
            settings.reconcile_boards(window, cx);
            assert_eq!(settings.form.repos, retained_repos);
            assert_eq!(input(settings, 1), retained_limit);
            assert_eq!(retained_limit.read(cx).value(), "99");
            assert!(settings.has_unsaved_changes(cx));
            assert!(
                settings
                    .navigation
                    .contains(&super::SettingsPage::Board("wiki"))
            );
            settings.select_initial_board("notes", cx);
            manager.update_web_board("notes", None).unwrap();
            let mut boards = crate::registry::included_registrations(cx);
            boards.retain(|board| board.id != "notes");
            cx.set_global(crate::registry::IncludedBoards(boards));
            settings.reconcile_boards(window, cx);
            assert!(settings.board_fields.is_empty());
            assert_eq!(settings.page, super::SettingsPage::General);
            assert_eq!(settings.form.repos, retained_repos);
            settings.save(window, cx).unwrap();
            let saved = SettingsFile::open(&path).unwrap();
            assert!(saved.web_board("notes").is_none());
            assert!(saved.web_board("wiki").is_some());
            assert!(saved.values().repos.contains(&"owner/unsaved".to_owned()));
            settings.restore_preview(window, cx).unwrap();
            assert!(
                SettingsFile::open(&path)
                    .unwrap()
                    .web_board("wiki")
                    .is_some()
            );
        })
        .unwrap();
}

fn add_wiki(manager: &mut SettingsFile, cx: &mut gpui_kit::App) {
    manager
        .update_web_board(
            "wiki",
            Some(
                json!({"url":"https://wiki.example"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .unwrap();
    let mut boards = crate::registry::included_registrations(cx);
    let mut added = super::navigation_tests::registration();
    added.id = "wiki";
    boards.push(added);
    cx.set_global(crate::registry::IncludedBoards(boards));
}

#[gpui_kit::test]
fn saved_values_fill_the_fields_but_secrets_stay_empty(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(input(settings, 0).read(cx).value(), "");
            assert_eq!(input(settings, 1).read(cx).value(), "3");
            assert_eq!(input(settings, 4).read(cx).value(), "");
            assert!(matches!(
                settings.board_fields[0].fields[4].control,
                Control::Secret { saved: true, .. }
            ));
            assert!(matches!(
                settings.board_fields[0].fields[5].control,
                Control::Secret { saved: false, .. }
            ));
            let tokens = Tokens::current(cx);
            let notes = settings.board_field_rows("notes", false, &tokens, cx);
            let other = settings.board_field_rows("other", false, &tokens, cx);
            assert!(notes.is_some() && other.is_none());
            assert!(!super::board_fields::changed(&settings.board_fields, cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn edited_fields_are_saved_and_untouched_ones_are_kept(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            input(settings, 0).update(cx, |input, cx| {
                input.set_value("https://api.example.com", window, cx)
            });
            input(settings, 1).update(cx, |input, cx| input.set_value("", window, cx));
            settings.set_field_toggle(0, 2, true);
            if let Control::Choice { choice, .. } = &settings.board_fields[0].fields[3].control {
                choice.update(cx, |choice, cx| {
                    choice.set_selected_value(&"dark".to_owned(), window, cx)
                });
            }
            input(settings, 5).update(cx, |input, cx| input.set_value(" new-key ", window, cx));
            settings.toggle_field_clear(0, 4, window, cx);
            assert!(!settings.save(window, cx).unwrap(), "no restart needed");
            assert_eq!(input(settings, 5).read(cx).value(), "");
        })
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    let notes = &saved["webBoards"]["notes"];
    assert_eq!(
        notes["settings"],
        json!({
            "endpoint": "https://api.example.com/",
            "compact": true,
            "theme": "dark",
            "kept": "unknown keys stay"
        })
    );
    assert_eq!(notes["secrets"], json!({"key": "new-key"}));
    assert_eq!(notes["url"], json!("https://notes.example"));
}

#[gpui_kit::test]
fn an_invalid_value_stops_the_save_and_names_the_field(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            input(settings, 1).update(cx, |input, cx| input.set_value("lots", window, cx));
            let error = settings.save(window, cx).unwrap_err();
            assert!(error.contains("limit"), "{error}");
        })
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("config.json")).unwrap(),
        CONFIG
    );
}

#[gpui_kit::test]
fn grants_default_off_persist_and_can_be_revoked(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(!settings.board_fields[0].permissions[0].on);
            settings.set_board_grant(0, "identity.settings", true);
            assert!(super::board_fields::changed(&settings.board_fields, cx));
            assert!(!settings.save(window, cx).unwrap());
            assert!(!super::board_fields::changed(&settings.board_fields, cx));
            let file = SettingsFile::open(&root.path().join("config.json")).unwrap();
            let drafts = super::board_fields::load(&settings.boards, &file, window, cx);
            assert!(drafts[0].permissions[0].on);
            settings.set_board_grant(0, "identity.settings", false);
            assert!(!settings.save(window, cx).unwrap());
        })
        .unwrap();
    let file = SettingsFile::open(&root.path().join("config.json")).unwrap();
    assert!(!file.board_permission(&fields().section, "identity.settings"));
    assert!(file.board_secret_present(&fields().section, "token"));
}

#[gpui_kit::test]
fn unsaved_grants_and_failed_validation_do_not_change_the_file(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.set_board_grant(0, "identity.settings", true);
            assert_eq!(
                std::fs::read_to_string(root.path().join("config.json")).unwrap(),
                CONFIG
            );
            input(settings, 1).update(cx, |input, cx| input.set_value("invalid", window, cx));
            assert!(settings.save(window, cx).is_err());
            assert!(super::board_fields::changed(&settings.board_fields, cx));
            assert!(
                !settings
                    .file
                    .board_permission(&fields().section, "identity.settings")
            );
        })
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("config.json")).unwrap(),
        CONFIG
    );
}

#[gpui_kit::test]
fn board_metadata_is_read_again_when_settings_opens(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    let revision = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let current = revision.clone();
    settings
        .update(cx, |settings, window, cx| {
            let board = settings.boards[0].clone().with_field_source(move || {
                let mut fields = fields();
                fields.permissions[0].key = format!(
                    "revision-{}.settings",
                    current.load(std::sync::atomic::Ordering::Relaxed)
                );
                Some(fields)
            });
            let first =
                super::board_fields::load(std::slice::from_ref(&board), &settings.file, window, cx);
            assert_eq!(first[0].permissions[0].key, "revision-1.settings");
            revision.store(2, std::sync::atomic::Ordering::Relaxed);
            let second = super::board_fields::load(&[board], &settings.file, window, cx);
            assert_eq!(second[0].permissions[0].key, "revision-2.settings");
            assert!(!second[0].permissions[0].on);
        })
        .unwrap();
}
