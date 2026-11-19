//! The inspector in the production shell: its key, a status bar button's
//! name and Developer settings….

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};
use subspace_dashboard::secondary;

use super::status::rooted_fixture;

fn frame(window: AnyWindowHandle, cx: &mut TestAppContext) {
    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn the_key_opens_the_inspector_and_names_a_status_bar_button(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = rooted_fixture(cx, root.path(), 2200., false);
    // The inspector's Root plugin joins roots created after it.
    cx.update(crate::inspector::init);
    cx.update_window(window, |_, window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(app.clone(), window, cx)
        });
    })
    .unwrap();
    cx.update_window(window, |_, window, cx| window.press(secondary!("f12"), cx))
        .unwrap();
    cx.run_until_parked();
    frame(window, cx);
    let button = cx
        .update_window(window, |_, window, cx| {
            assert!(crate::inspector::is_open(window, cx));
            window.find("status-help").bounds()
        })
        .unwrap();
    cx.update_window(window, |_, window, cx| {
        window.simulate_mouse_move(button.center(), cx);
    })
    .unwrap();
    frame(window, cx);
    let (name, bounds) = cx.update(|cx| crate::inspector::reported(cx)).unwrap();
    assert_eq!(name.as_deref(), Some("icon_button::IconButton"));
    assert!(button.contains(&bounds.center()));
}

#[gpui_kit::test]
fn developer_settings_open_settings_at_the_developer_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = rooted_fixture(cx, root.path(), 2200., false);
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::inspector::OpenDeveloperSettings), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let shown = app.read_with(cx, |app, cx| {
        app.settings
            .as_ref()
            .is_some_and(|settings| settings.read(cx).shows_developer_page())
    });
    assert!(shown);
}
