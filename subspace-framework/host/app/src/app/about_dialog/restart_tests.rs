use std::path::Path;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, px, size};
use subspace_dashboard::prefs;

use super::super::{OpenAbout, OpenSettings, SubspaceApp, toolbar::tests::status::rooted_fixture};
use super::update_panel::seed_installed;
use crate::settings::CONFIRM_QUIT_KEY;

fn click(window: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn installed_update(
    root: &Path,
    confirm: &str,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<SubspaceApp>) {
    let (window, app) = rooted_fixture(cx, root, 1600., false);
    cx.simulate_window_resize(window, size(px(1600.), px(1000.)));
    cx.update(|cx| {
        cx.set_reduce_motion(true);
        prefs::set(cx, CONFIRM_QUIT_KEY, confirm);
        seed_installed("1.2.0", cx);
    });
    (window, app)
}

fn open_about(root: &Path, confirm: &str, cx: &mut TestAppContext) -> AnyWindowHandle {
    let (window, app) = installed_update(root, confirm, cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.open_about(&OpenAbout, window, cx));
    })
    .unwrap();
    window
}

fn open_settings(root: &Path, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<SubspaceApp>) {
    std::fs::write(root.join("config.json"), "{}").unwrap();
    let (window, app) = installed_update(root, "false", cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.open_settings(&OpenSettings, window, cx));
    })
    .unwrap();
    (window, app)
}

#[gpui_kit::test]
fn about_restart_reaches_the_platform_without_quit_confirmation(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open_about(root.path(), "false", cx);
    let mut restart = cx.expect_restart();

    click(window, "restart-to-update", cx);

    assert!(restart.try_recv().unwrap().is_some());
}

#[gpui_kit::test]
fn about_restart_closes_about_before_showing_the_quit_confirmation(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open_about(root.path(), "true", cx);
    let mut restart = cx.expect_restart();

    click(window, "restart-to-update", cx);
    assert!(restart.try_recv().unwrap().is_none());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        assert_eq!(
            window.find("confirm-quit").label(),
            Some("Quit and restart")
        );
    })
    .unwrap();

    click(window, "confirm-quit", cx);

    assert!(restart.try_recv().unwrap().is_some());
}

#[gpui_kit::test]
fn clean_settings_restart_closes_the_editor_and_reaches_the_platform(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open_settings(root.path(), cx);
    let mut restart = cx.expect_restart();

    click(window, "settings-update-action", cx);

    assert!(restart.try_recv().unwrap().is_some());
    assert!(app.read_with(cx, |app, _| app.settings.is_none()));
}

#[gpui_kit::test]
fn dirty_settings_restart_honours_cancel_discard_and_save(cx: &mut TestAppContext) {
    for save in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (window, app) = open_settings(root.path(), cx);
        let mut restart = cx.expect_restart();

        click(window, "automatic-board-refresh", cx);
        click(window, "settings-update-action", cx);
        assert!(restart.try_recv().unwrap().is_none());
        cx.update_window(window, |_, window, cx| window.press("escape", cx))
            .unwrap();
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.settings.is_some()));
        assert!(restart.try_recv().unwrap().is_none());

        click(window, "settings-update-action", cx);
        click(
            window,
            if save {
                "settings-save-and-exit"
            } else {
                "settings-discard"
            },
            cx,
        );
        assert!(restart.try_recv().unwrap().is_some());
        assert!(app.read_with(cx, |app, _| app.settings.is_none()));
        let config = subspace_core::config::load(&root.path().join("config.json")).0;
        assert_eq!(config.auto_refresh, !save);
    }
}
