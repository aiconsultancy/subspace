use std::sync::Arc;

use gpui_kit::component::Theme;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};

use super::{Fixture, preview_light_theme};
use crate::settings::CONFIRM_QUIT_KEY;

fn show_log(fixture: &Fixture, cx: &mut TestAppContext) {
    cx.update_window(fixture.window, |_, window, cx| {
        window.render_frame(cx);
        window.click(
            ElementId::NamedChild(
                Arc::new("settings-update-copy-details".into()),
                "show-log".into(),
            ),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
}

fn seed_failure(cx: &mut TestAppContext) {
    cx.update(crate::app::about_dialog::update_panel::seed_failure);
    cx.run_until_parked();
}

fn assert_log_open(fixture: &Fixture, expected: bool, cx: &TestAppContext) {
    assert_eq!(
        fixture.app.read_with(cx, |app, _| matches!(
            app.main,
            crate::app::MainSurface::Logs { .. }
        )),
        expected
    );
}

#[gpui_kit::test]
fn clean_settings_show_log_closes_the_owner_and_allows_settings_to_reopen(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    seed_failure(cx);
    show_log(&fixture, cx);
    fixture.assert_closed(cx);
    assert_log_open(&fixture, true, cx);
    cx.update_window(fixture.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("log-view").visible());
        fixture.app.update(cx, |app, cx| {
            app.open_settings(&crate::app::OpenSettings, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(fixture.dialogs(cx), 1);
    assert!(fixture.app.read_with(cx, |app, _| app.settings.is_some()));
}

#[gpui_kit::test]
fn cancelling_show_log_keeps_the_draft_preview_and_clears_pending_navigation(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    seed_failure(cx);
    fixture.click("confirm-before-quitting", cx);
    let settings = fixture.settings(cx);
    preview_light_theme(&fixture, cx);
    show_log(&fixture, cx);
    assert_eq!(fixture.dialogs(cx), 2);
    assert_log_open(&fixture, false, cx);
    fixture.press("escape", cx);
    assert_eq!(fixture.settings(cx), settings);
    assert_eq!(fixture.dialogs(cx), 1);
    assert!(settings.read_with(cx, |settings, cx| settings.has_unsaved_changes(cx)));
    cx.update(|cx| assert_eq!(Theme::global(cx).theme_name().as_ref(), "Solarized Light"));
    fixture.press("escape", cx);
    fixture.click("settings-discard", cx);
    fixture.assert_closed(cx);
    assert_log_open(&fixture, false, cx);
    cx.update(|cx| assert_eq!(Theme::global(cx).theme_name().as_ref(), "Solarized Dark"));
}

#[gpui_kit::test]
fn dirty_show_log_saves_or_discards_before_opening_the_log(cx: &mut TestAppContext) {
    for save in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let fixture = Fixture::open(root.path(), cx);
        seed_failure(cx);
        fixture.click("confirm-before-quitting", cx);
        show_log(&fixture, cx);
        assert_eq!(fixture.dialogs(cx), 2);
        assert_log_open(&fixture, false, cx);
        fixture.click(
            if save {
                "settings-save-and-exit"
            } else {
                "settings-discard"
            },
            cx,
        );
        fixture.assert_closed(cx);
        assert_log_open(&fixture, true, cx);
        let state = subspace_core::state::State::load(&root.path().join("state.json"));
        assert_eq!(state.get(CONFIRM_QUIT_KEY) == Some("true"), save);
    }
}

#[gpui_kit::test]
fn failed_show_log_save_returns_to_the_draft_without_a_later_unrequested_navigation(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    seed_failure(cx);
    fixture.click("automatic-board-refresh", cx);
    let settings = fixture.settings(cx);
    std::fs::write(root.path().join("config.json"), "{\"external-edit\":true}").unwrap();
    show_log(&fixture, cx);
    fixture.click("settings-save-and-exit", cx);
    assert_eq!(fixture.dialogs(cx), 1);
    assert_eq!(fixture.settings(cx), settings);
    assert_log_open(&fixture, false, cx);
    assert!(settings.read_with(cx, |settings, cx| settings.has_unsaved_changes(cx)));
    cx.update_window(fixture.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("settings-error-message")
                .label()
                .unwrap()
                .contains("changed externally")
        );
    })
    .unwrap();
    std::fs::write(root.path().join("config.json"), "{}").unwrap();
    fixture.click("settings-save", cx);
    fixture.assert_closed(cx);
    assert_log_open(&fixture, false, cx);
}
