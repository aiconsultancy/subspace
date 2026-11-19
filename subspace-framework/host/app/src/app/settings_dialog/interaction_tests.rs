//! Production dialog input must preserve its draft until a leave decision.

mod board_editor;
mod geometry;
mod show_log;

use std::path::Path;

use gpui_kit::base::test_support::snapshots;
use gpui_kit::component::Theme;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Role, TestAppContext, px, size};
use subspace_dashboard::density::DensityPreview;
use subspace_dashboard::prefs;

use super::super::SubspaceApp;
use super::super::toolbar::tests::status::rooted_fixture;
use crate::settings::{CONFIRM_QUIT_KEY, Settings};
use crate::theme::{self, THEME_PREF};

struct Fixture {
    window: AnyWindowHandle,
    app: Entity<SubspaceApp>,
}

#[gpui_kit::test]
fn notification_master_precedes_copilot_and_only_applies_on_save(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    cx.update(|cx| {
        prefs::set(cx, THEME_PREF, "Cupertino Dark");
        theme::initialize(cx);
    });
    cx.simulate_window_resize(fixture.window, size(px(1280.), px(800.)));
    cx.update_window(fixture.window, |_, window, cx| {
        window.render_frame(cx);
        let navigation = window.find("settings-app:notifications").bounds();
        eprintln!("Notifications capture target: {navigation:?}");
    })
    .unwrap();
    fixture.click("settings-app:notifications", cx);
    cx.update_window(fixture.window, |_, window, cx| {
        window.render_frame(cx);
        let master = window.find("notification-sounds");
        assert_eq!(master.label(), Some("Play notification sounds"));
        assert!(master.bounds().top() < window.find("notification-completion").bounds().top());
    })
    .unwrap();
    fixture.click("notification-sounds", cx);
    cx.update(|cx| assert!(subspace_dashboard::notifications::sounds_enabled(cx)));
    fixture.click("settings-save", cx);
    fixture.assert_closed(cx);
    cx.update(|cx| assert!(!subspace_dashboard::notifications::sounds_enabled(cx)));
}

impl Fixture {
    fn open(root: &Path, cx: &mut TestAppContext) -> Self {
        std::fs::write(root.join("config.json"), "{}").unwrap();
        let (window, app) = rooted_fixture(cx, root, 2200., false);
        cx.update(|cx| {
            cx.set_reduce_motion(true);
            prefs::set(cx, THEME_PREF, "Solarized Dark");
            theme::initialize(cx);
            prefs::save(cx).unwrap();
        });
        cx.simulate_window_resize(window, size(px(2200.), px(1100.)));
        let fixture = Self { window, app };
        // Linux reserves plain Ctrl keys for the focused terminal.
        let settings_key = if cfg!(target_os = "linux") {
            "ctrl-shift-,"
        } else {
            "secondary-,"
        };
        fixture.press(settings_key, cx);
        assert!(fixture.app.read_with(cx, |app, _| app.settings.is_some()));
        fixture
    }

    fn click(&self, id: &'static str, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.click(id, cx))
            .unwrap();
        cx.run_until_parked();
    }

    fn press(&self, key: &str, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.press(key, cx))
            .unwrap();
        cx.run_until_parked();
    }

    fn settings(&self, cx: &TestAppContext) -> Entity<Settings> {
        self.app
            .read_with(cx, |app, _| app.settings.clone().unwrap())
    }

    fn dialogs(&self, cx: &mut TestAppContext) -> usize {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            snapshots(window)
                .iter()
                .filter(|element| matches!(element.role(), Some(Role::Dialog | Role::AlertDialog)))
                .count()
        })
        .unwrap()
    }

    fn assert_closed(&self, cx: &mut TestAppContext) {
        assert!(self.app.read_with(cx, |app, _| app.settings.is_none()));
        assert_eq!(self.dialogs(cx), 0);
        cx.update(|cx| assert!(!cx.global::<DensityPreview>().0));
    }

    fn close_icon(&self, layer: usize, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window.within(format!("dialog-{layer}")).click("close", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn clean_and_reverted_settings_escape_without_a_confirmation(cx: &mut TestAppContext) {
    for reverted in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let fixture = Fixture::open(root.path(), cx);
        if reverted {
            fixture.click("confirm-before-quitting", cx);
            fixture.click("confirm-before-quitting", cx);
        }
        assert!(
            !fixture
                .settings(cx)
                .read_with(cx, |settings, cx| settings.has_unsaved_changes(cx))
        );
        fixture.press("escape", cx);
        fixture.assert_closed(cx);
    }
}

#[gpui_kit::test]
fn every_dirty_close_route_preserves_the_editor_and_escape_returns_to_it(cx: &mut TestAppContext) {
    for route in ["escape", "cancel", "close"] {
        let root = tempfile::tempdir().unwrap();
        let fixture = Fixture::open(root.path(), cx);
        fixture.click("confirm-before-quitting", cx);
        let settings = fixture.settings(cx);
        match route {
            "escape" => fixture.press("escape", cx),
            "cancel" => fixture.click("settings-cancel", cx),
            _ => fixture.close_icon(0, cx),
        }
        assert_eq!(fixture.dialogs(cx), 2, "{route} must ask before closing");
        assert_eq!(fixture.settings(cx), settings);
        fixture.press("escape", cx);
        assert_eq!(fixture.dialogs(cx), 1, "Escape dismisses only the prompt");
        assert_eq!(fixture.settings(cx), settings);
        cx.update_window(fixture.window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("confirm-before-quitting").checked(), Some(true));
        })
        .unwrap();
        fixture.press("escape", cx);
        assert_eq!(
            fixture.dialogs(cx),
            2,
            "a later close request may ask again"
        );
        fixture.click("settings-discard", cx);
        fixture.assert_closed(cx);
    }
}

#[gpui_kit::test]
fn confirmation_close_icon_returns_to_the_same_dirty_editor(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    fixture.click("confirm-before-quitting", cx);
    let settings = fixture.settings(cx);
    fixture.press("escape", cx);
    fixture.close_icon(1, cx);
    assert_eq!(fixture.dialogs(cx), 1);
    assert_eq!(fixture.settings(cx), settings);
    assert!(settings.read_with(cx, |settings, cx| settings.has_unsaved_changes(cx)));
}

#[gpui_kit::test]
fn repeated_escape_before_the_deferred_prompt_does_not_stack_confirmations(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    fixture.click("confirm-before-quitting", cx);
    let settings = fixture.settings(cx);
    cx.update_window(fixture.window, |_, window, cx| {
        window.press("escape", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(fixture.dialogs(cx), 2);
    assert_eq!(fixture.settings(cx), settings);
    fixture.press("escape", cx);
    assert_eq!(fixture.dialogs(cx), 1);
    assert!(settings.read_with(cx, |settings, cx| settings.has_unsaved_changes(cx)));
}

fn preview_light_theme(fixture: &Fixture, cx: &mut TestAppContext) {
    fixture.click("settings-app:appearance", cx);
    cx.update_window(fixture.window, |_, window, cx| {
        let theme = snapshots(window)
            .into_iter()
            .find(|element| element.label() == Some("Theme"))
            .unwrap();
        window.click(theme.path().last().unwrap().clone(), cx);
        window.input("Solarized Light", cx);
    })
    .unwrap();
    cx.run_until_parked();
    fixture.press("down", cx);
    fixture.press("enter", cx);
    cx.update(|cx| assert_eq!(Theme::global(cx).theme_name().as_ref(), "Solarized Light"));
}

#[gpui_kit::test]
fn discard_restores_the_opening_preview_without_writing_either_file(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    let config = std::fs::read(root.path().join("config.json")).unwrap();
    let state = std::fs::read(root.path().join("state.json")).unwrap();
    preview_light_theme(&fixture, cx);
    fixture.press("escape", cx);
    cx.update(|cx| assert_eq!(Theme::global(cx).theme_name().as_ref(), "Solarized Light"));
    fixture.click("settings-discard", cx);
    fixture.assert_closed(cx);
    cx.update(|cx| assert_eq!(Theme::global(cx).theme_name().as_ref(), "Solarized Dark"));
    assert_eq!(
        std::fs::read(root.path().join("config.json")).unwrap(),
        config
    );
    assert_eq!(
        std::fs::read(root.path().join("state.json")).unwrap(),
        state
    );
}

#[gpui_kit::test]
fn save_and_exit_persists_config_and_preferences_through_pointer_and_enter(
    cx: &mut TestAppContext,
) {
    for keyboard in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let fixture = Fixture::open(root.path(), cx);
        fixture.click("automatic-board-refresh", cx);
        fixture.click("confirm-before-quitting", cx);
        fixture.press("escape", cx);
        if keyboard {
            fixture.press("enter", cx);
        } else {
            fixture.click("settings-save-and-exit", cx);
        }
        fixture.assert_closed(cx);
        let config = subspace_core::config::load(&root.path().join("config.json")).0;
        assert!(!config.auto_refresh);
        let state = subspace_core::state::State::load(&root.path().join("state.json"));
        assert_eq!(state.get(CONFIRM_QUIT_KEY), Some("true"));
    }
}

#[gpui_kit::test]
fn failed_save_and_exit_keeps_the_draft_shows_the_error_and_allows_retry(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    fixture.click("automatic-board-refresh", cx);
    let settings = fixture.settings(cx);
    std::fs::write(root.path().join("config.json"), "{\"external-edit\":true}").unwrap();
    fixture.press("escape", cx);
    fixture.click("settings-save-and-exit", cx);
    assert_eq!(fixture.dialogs(cx), 1);
    assert_eq!(fixture.settings(cx), settings);
    assert!(settings.read_with(cx, |settings, cx| settings.has_unsaved_changes(cx)));
    cx.update_window(fixture.window, |_, window, cx| {
        window.render_frame(cx);
        let error = window.find("settings-error-message");
        assert!(error.visible());
        assert!(error.label().unwrap().contains("changed externally"));
        assert_eq!(
            window.find("automatic-board-refresh").checked(),
            Some(false)
        );
    })
    .unwrap();
    std::fs::write(root.path().join("config.json"), "{}").unwrap();
    fixture.click("settings-save", cx);
    fixture.assert_closed(cx);
    assert!(
        !subspace_core::config::load(&root.path().join("config.json"))
            .0
            .auto_refresh
    );
}
