//! Production status commands must use the shell's existing action handlers.

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use subspace_dashboard::PaletteMode;

use super::{SubspaceApp, open_fixture, settle};

#[gpui_kit::test]
fn status_pointer_commands_open_the_existing_shell_surfaces(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = rooted_fixture(cx, root.path(), 2200., false);
    cx.update_window(window, |_, window, _| {
        let label = format!("{} Commands", crate::app::palette_key());
        assert_eq!(window.find("status-commands").label(), Some(label.as_str()));
    })
    .unwrap();
    click(&window, cx, "status-commands");
    assert!(app.read_with(cx, |app, _| {
        app.session
            .as_ref()
            .is_some_and(|session| session.mode == PaletteMode::Commands)
    }));
    press(&window, cx, "escape");
    click(&window, cx, "status-help");
    assert!(app.read_with(cx, |app, _| app.help_open));
    press(&window, cx, "escape");
    click(&window, cx, "status-version");
    cx.update_window(window, |_, window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "version must open About, not install an update"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn status_commands_are_tab_reachable_and_activate_with_enter_and_space(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = rooted_fixture(cx, root.path(), 2200., false);
    tab_to(&window, cx, "status-commands");
    press(&window, cx, "enter");
    assert!(app.read_with(cx, |app, _| app.session.is_some()));
    press(&window, cx, "escape");
    tab_to(&window, cx, "status-help");
    press(&window, cx, "space");
    assert!(app.read_with(cx, |app, _| app.help_open));
    press(&window, cx, "escape");
    tab_to(&window, cx, "status-version");
    press(&window, cx, "enter");
    cx.update_window(window, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
    })
    .unwrap();
}

pub(in crate::app) fn rooted_fixture(
    cx: &mut TestAppContext,
    root: &std::path::Path,
    width: f32,
    labels: bool,
) -> (AnyWindowHandle, Entity<SubspaceApp>) {
    let handle = open_fixture(cx, root, "horizontal", width, labels);
    let app = handle.update(cx, |_, _, cx| cx.entity()).unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(app.clone(), window, cx)
        });
    })
    .unwrap();
    settle(&handle, cx);
    (handle.into(), app)
}

fn click(window: &AnyWindowHandle, cx: &mut TestAppContext, id: &'static str) {
    cx.update_window(*window, |_, window, cx| window.click(id, cx))
        .unwrap();
    cx.run_until_parked();
}

fn press(window: &AnyWindowHandle, cx: &mut TestAppContext, key: &str) {
    cx.update_window(*window, |_, window, cx| window.press(key, cx))
        .unwrap();
    cx.run_until_parked();
}

fn tab_to(window: &AnyWindowHandle, cx: &mut TestAppContext, id: &'static str) {
    for _ in 0..64 {
        press(window, cx, "tab");
        if cx
            .update_window(*window, |_, window, _| {
                window.find(id).focused() == Some(true)
            })
            .unwrap()
        {
            return;
        }
    }
    panic!("{id} must be a native Tab stop");
}
