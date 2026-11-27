use std::path::Path;

use gpui_kit::{TestAppContext, WindowHandle};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_dashboard::prefs;
use subspace_dashboard::refresh::{self, Preferences};

use super::{Settings, SettingsPage};
use crate::theme::{self, THEME_PREF};

fn open_settings(
    root: &Path,
    saved: Option<Preferences>,
    cx: &mut TestAppContext,
) -> WindowHandle<Settings> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        prefs::set(cx, THEME_PREF, "Solarized Light");
        theme::initialize(cx);
        let mut remote = super::navigation_tests::registration();
        remote.settings.background_updates = true;
        let mut other = remote.clone();
        other.id = "other-remote";
        let mut local = super::navigation_tests::registration();
        local.id = "local-only";
        cx.set_global(crate::registry::IncludedBoards(vec![remote, local, other]));
        if let Some(saved) = saved {
            saved.save("future", cx);
        }
        prefs::save(cx).unwrap();
    });
    let path = root.join("config.json");
    std::fs::write(&path, "{}").unwrap();
    let file = SettingsFile::open(&path).unwrap();
    cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx))
}

fn disabled_override() -> Preferences {
    Preferences {
        enabled: false,
        interval_seconds: Some(300),
    }
}

fn edit_draft(settings: WindowHandle<Settings>, cx: &mut TestAppContext) {
    settings
        .update(cx, |settings, window, cx| {
            settings.background_updates[0].enabled = false;
            settings.background_updates[0]
                .interval
                .update(cx, |select, cx| {
                    select.set_selected_value(&"5 minutes".to_owned(), window, cx);
                });
        })
        .unwrap();
}

#[gpui_kit::test]
fn background_update_controls_follow_capability_and_main_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), None, cx);
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(settings.background_updates.len(), 2);
            assert_eq!(settings.background_updates[0].id, "future");
            assert_eq!(settings.background_updates[1].id, "other-remote");
            for (page, visible) in [
                (SettingsPage::Board("future"), true),
                (SettingsPage::Board("local-only"), false),
                (SettingsPage::Board("other-remote"), true),
                (SettingsPage::General, false),
                (SettingsPage::Notifications, false),
                (
                    SettingsPage::BoardSection {
                        board: "future",
                        path: vec![0],
                    },
                    false,
                ),
            ] {
                assert_eq!(
                    settings
                        .background_update_controls(&page, false, cx)
                        .is_some(),
                    visible,
                    "{page:?}"
                );
            }
        })
        .unwrap();
}

#[gpui_kit::test]
fn saved_background_updates_apply_without_restart_and_survive_reload(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), None, cx);
    edit_draft(settings, cx);
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(refresh::preferences("future", cx), Preferences::default());
            assert!(!settings.save(window, cx).unwrap(), "no restart needed");
            assert_eq!(refresh::preferences("future", cx), disabled_override());
            assert_eq!(
                refresh::preferences("other-remote", cx),
                Preferences::default()
            );
        })
        .unwrap();
    let reopened = open_settings(root.path(), None, cx);
    reopened
        .update(cx, |settings, _, cx| {
            assert_eq!(refresh::preferences("future", cx), disabled_override());
            assert_eq!(
                settings.background_updates[0].preferences(cx),
                disabled_override()
            );
            assert_eq!(
                settings.background_updates[1].preferences(cx),
                Preferences::default()
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn cancelled_background_updates_write_nothing(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), None, cx);
    let state_file = root.path().join("state.json");
    let original = std::fs::read(&state_file).unwrap();
    edit_draft(settings, cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(refresh::preferences("future", cx), Preferences::default());
        })
        .unwrap();
    assert_eq!(std::fs::read(state_file).unwrap(), original);
}

#[gpui_kit::test]
fn failed_settings_save_keeps_background_preferences(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), None, cx);
    let state_file = root.path().join("state.json");
    std::fs::remove_file(&state_file).unwrap();
    std::fs::create_dir(&state_file).unwrap();
    edit_draft(settings, cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.save(window, cx).is_err());
            assert_eq!(refresh::preferences("future", cx), Preferences::default());
            assert!(
                super::background_updates::save(&mut settings.background_updates, |_| {}, cx)
                    .is_err()
            );
            assert_eq!(refresh::preferences("future", cx), Preferences::default());
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(refresh::preferences("future", cx), Preferences::default());
        })
        .unwrap();
}

#[gpui_kit::test]
fn untouched_background_drafts_preserve_concurrent_changes(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), None, cx);
    edit_draft(settings, cx);
    settings
        .update(cx, |settings, window, cx| {
            let concurrent = Preferences {
                enabled: true,
                interval_seconds: Some(75),
            };
            concurrent.save("other-remote", cx);
            settings.save(window, cx).unwrap();
            assert_eq!(refresh::preferences("future", cx), disabled_override());
            assert_eq!(refresh::preferences("other-remote", cx), concurrent);
        })
        .unwrap();
}

#[gpui_kit::test]
fn background_interval_choices_preserve_defaults_and_saved_custom_values(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let saved = Preferences {
        enabled: true,
        interval_seconds: Some(86_400),
    };
    let settings = open_settings(root.path(), Some(saved), cx);
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(settings.background_updates[0].preferences(cx), saved);
            settings.save(window, cx).unwrap();
            assert_eq!(refresh::preferences("future", cx), saved);
            for (label, seconds) in [
                ("Default", None),
                ("30 seconds", Some(30)),
                ("1 minute", Some(60)),
                ("2 minutes", Some(120)),
                ("5 minutes", Some(300)),
                ("10 minutes", Some(600)),
                ("15 minutes", Some(900)),
                ("30 minutes", Some(1800)),
                ("1 hour", Some(3600)),
            ] {
                settings.background_updates[0]
                    .interval
                    .update(cx, |select, cx| {
                        select.set_selected_value(&label.to_owned(), window, cx);
                    });
                assert_eq!(
                    settings.background_updates[0]
                        .preferences(cx)
                        .interval_seconds,
                    seconds
                );
            }
            settings.save(window, cx).unwrap();
            assert_eq!(
                refresh::preferences("future", cx).interval_seconds,
                Some(3600)
            );
        })
        .unwrap();
}
