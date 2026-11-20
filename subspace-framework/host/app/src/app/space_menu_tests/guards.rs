//! What closing and the menu guard against: a config file edited outside
//! the app, and a web Space's native view drawing over the open menu.

use gpui_kit::TestAppContext;

use super::{CLOSE_SPACE, IDS, choose, ids, menu_item, open, saved_boards, settle};

#[gpui_kit::test]
fn closing_never_leaves_no_space_on_after_an_outside_edit(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    // Edited outside the app: Beta off, Gamma back to its default, off.
    let path = root.path().join("config.json");
    std::fs::write(&path, r#"{"boards": {"beta": false}}"#).unwrap();
    choose(window, "alpha", CLOSE_SPACE, cx);
    assert_eq!(
        saved_boards(root.path()),
        Some(serde_json::json!({"beta": false}))
    );
    assert_eq!(ids(&app, cx), IDS);
    app.read_with(cx, |app, _| {
        let toast = &app.toasts.last().unwrap().toast.text;
        assert_eq!(
            toast,
            "Couldn't close Alpha: it is the only Space the config file leaves on"
        );
    });
}

#[gpui_kit::test]
fn an_open_space_menu_covers_the_board(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    assert!(app.read_with(cx, |app, _| !app.cover.covered));
    menu_item(window, "beta", CLOSE_SPACE, cx);
    assert!(app.read_with(cx, |app, _| app.cover.covered));
    cx.simulate_keystrokes(window, "escape");
    settle(window, cx);
    assert!(app.read_with(cx, |app, _| !app.cover.covered));
}
