use gpui_kit::{TestAppContext, WindowHandle};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_core::state::State;
use subspace_dashboard::notifications::SOUNDS_KEY;
use subspace_dashboard::notifications::{self, Mode, Preferences};
use subspace_dashboard::prefs;

use super::Settings;
use crate::notifications::{
    COMPLETION_KEY, NotificationPreferences, OS_KEY, SOUND_KEY, WAITING_KEY,
};
use crate::theme::{self, THEME_PREF};

fn open_settings(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<Settings>) {
    open_settings_with(cx, Preferences::default())
}

/// Settings over one notifying board, "future", that declares `defaults`.
fn open_settings_with(
    cx: &mut TestAppContext,
    defaults: Preferences,
) -> (tempfile::TempDir, WindowHandle<Settings>) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        prefs::set(cx, THEME_PREF, "Solarized Light");
        let mut source = super::navigation_tests::registration();
        source.settings.notifications = true;
        source.settings.notification_defaults = defaults;
        crate::notifications::install_defaults(std::slice::from_ref(&source), cx);
        cx.set_global(crate::registry::IncludedBoards(vec![source]));
        theme::initialize(cx);
        prefs::save(cx).unwrap();
    });
    let path = root.path().join("config.json");
    std::fs::write(&path, "{}").unwrap();
    let file = SettingsFile::open(&path).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    (root, settings)
}

fn changed_preferences() -> NotificationPreferences {
    NotificationPreferences {
        sounds: false,
        completion: false,
        waiting: false,
        os: true,
        sound: true,
    }
}

#[gpui_kit::test]
fn master_switch_edits_a_draft_and_save_preserves_each_space_choice(cx: &mut TestAppContext) {
    use gpui_kit::AppContext as _;
    use gpui_kit::test::TestWindowExt as _;
    let root = tempfile::tempdir().unwrap();
    let settings = super::organization_tests::open_settings(root.path(), cx);
    cx.update(|cx| prefs::set(cx, "notifications.future.sound", "Glass"));
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("settings-app:notifications", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("notification-sounds").visible());
        window.click("notification-sounds", cx);
    })
    .unwrap();
    cx.run_until_parked();
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(settings.page, super::SettingsPage::Notifications);
            assert!(!settings.notifications.sounds);
            assert!(settings.has_unsaved_changes(cx));
            assert!(
                notifications::sounds_enabled(cx),
                "unsaved edits must not mute playback"
            );
            settings.save(window, cx).unwrap();
            assert!(!notifications::sounds_enabled(cx));
            assert!(!settings.has_unsaved_changes(cx));
            assert_eq!(
                prefs::get(cx, "notifications.future.sound").as_deref(),
                Some("Glass")
            );
            prefs::init_file(Some(root.path().join("state.json")), cx);
            assert!(!NotificationPreferences::load(cx).sounds);
        })
        .unwrap();
}

#[gpui_kit::test]
fn an_unrelated_save_leaves_a_board_on_its_declared_defaults(cx: &mut TestAppContext) {
    let declared = Preferences {
        mode: Mode::System,
        ..Preferences::default()
    };
    let (_root, settings) = open_settings_with(cx, declared);
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(settings.sources[0].preferences(cx), declared);
            settings.notifications = changed_preferences();
            settings.save(window, cx).unwrap();
            assert_eq!(
                prefs::get(cx, "notifications.future.mode"),
                None,
                "a Save that leaves the board alone stores no choice for it"
            );
            assert_eq!(notifications::preferences("future", cx), declared);
            settings.sources[0].mode.update(cx, |select, cx| {
                select.set_selected_value(&"Off".to_owned(), window, cx)
            });
            settings.save(window, cx).unwrap();
            assert_eq!(
                notifications::preferences("future", cx).mode,
                Mode::Off,
                "a chosen Off holds over the declared default"
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn cancelling_notification_edits_writes_no_preferences(cx: &mut TestAppContext) {
    let (root, settings) = open_settings(cx);
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(
                settings.notifications,
                NotificationPreferences {
                    sounds: true,
                    completion: true,
                    waiting: true,
                    os: false,
                    sound: false,
                }
            );
            settings.notifications = changed_preferences();
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(
                [SOUNDS_KEY, COMPLETION_KEY, WAITING_KEY, OS_KEY, SOUND_KEY]
                    .map(|key| prefs::get(cx, key)),
                [None, None, None, None, None]
            );
        })
        .unwrap();
    let saved = State::load(&root.path().join("state.json"));
    assert_eq!(
        [SOUNDS_KEY, COMPLETION_KEY, WAITING_KEY, OS_KEY, SOUND_KEY].map(|key| saved.get(key)),
        [None, None, None, None, None]
    );
}

#[gpui_kit::test]
fn a_failed_save_does_not_apply_notification_edits(cx: &mut TestAppContext) {
    let (root, settings) = open_settings(cx);
    let state_file = root.path().join("state.json");
    std::fs::remove_file(&state_file).unwrap();
    std::fs::create_dir(&state_file).unwrap();
    settings
        .update(cx, |settings, window, cx| {
            settings.notifications = changed_preferences();
            settings.sources[0].mode.update(cx, |select, cx| {
                select.set_selected_value(&"OS".to_owned(), window, cx)
            });
            assert!(settings.save(window, cx).is_err());
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(
                [SOUNDS_KEY, COMPLETION_KEY, WAITING_KEY, OS_KEY, SOUND_KEY]
                    .map(|key| prefs::get(cx, key)),
                [None, None, None, None, None]
            );
            assert_eq!(
                subspace_dashboard::notifications::preferences("future", cx).mode,
                subspace_dashboard::notifications::Mode::Off
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn saving_notification_edits_applies_master_and_copilot_preferences(cx: &mut TestAppContext) {
    let (root, settings) = open_settings(cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.notifications = changed_preferences();
            assert!(!settings.save(window, cx).unwrap());
            prefs::save(cx).unwrap();
        })
        .unwrap();
    let saved = State::load(&root.path().join("state.json"));
    assert_eq!(
        [SOUNDS_KEY, COMPLETION_KEY, WAITING_KEY, OS_KEY, SOUND_KEY]
            .map(|key| saved.get(key).unwrap()),
        ["0", "0", "0", "1", "1"]
    );
}

#[gpui_kit::test]
fn source_choices_cancel_and_off_test_do_not_write_but_save_survives_reload(
    cx: &mut TestAppContext,
) {
    use subspace_dashboard::notifications::{self, Mode, Sound, Style};
    let (root, settings) = open_settings(cx);
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(settings.sources.len(), 1);
            let entity = cx.entity();
            window.replace_root(cx, |window, cx| {
                gpui_kit::base::Root::new(entity, window, cx)
            });
            settings.preview_notification(0, false, window, cx);
            assert!(settings.notice.as_deref().unwrap().contains("Off"));
            settings.sources[0].mode.update(cx, |select, cx| {
                select.set_selected_value(&"In-app".to_owned(), window, cx)
            });
            settings.preview_notification(0, false, window, cx);
            assert_eq!(notifications::preferences("future", cx).mode, Mode::Off);
            assert!(
                settings
                    .notice
                    .as_deref()
                    .unwrap()
                    .contains("Test requested")
            );
            settings.sources[0].mode.update(cx, |select, cx| {
                select.set_selected_value(&"Both".to_owned(), window, cx)
            });
            settings.sources[0].style.update(cx, |select, cx| {
                select.set_selected_value(&"Persistent".to_owned(), window, cx)
            });
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(notifications::preferences("future", cx).mode, Mode::Off);
            settings.save(window, cx).unwrap();
            let saved = notifications::preferences("future", cx);
            assert_eq!(
                (saved.mode, saved.style, saved.sound),
                (Mode::Both, Style::Persistent, Sound::Silent)
            );
        })
        .unwrap();
    cx.update(|cx| {
        prefs::init_file(Some(root.path().join("state.json")), cx);
        assert_eq!(notifications::preferences("future", cx).mode, Mode::Both);
        assert_eq!(
            notifications::preferences("future", cx).style,
            Style::Persistent
        );
    });
}
