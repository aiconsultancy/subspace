use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use super::open;

#[gpui_kit::test]
fn switching_boards_while_help_is_zoomed_never_focuses_the_hidden_board(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("help", cx);
        window.click("zoom-in", cx);
        window.dispatch_action(Box::new(crate::app::keys::ShowDashboard { index: 1 }), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let app = app.read(cx);
        assert_eq!(app.dashboard().unwrap().id, "help-beta");
        assert_eq!(
            window.find("help-dock-board-title").label(),
            Some("Beta board")
        );
        assert!(window.find("zoom-out").visible());
        assert!(!app.dashboard().unwrap().focus_handle(cx).is_focused(window));
    })
    .unwrap();
}

#[gpui_kit::test]
fn shortcut_filter_supports_keyboard_search_clear_and_escape(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("help", cx);
        window.press("secondary-alt-f", cx);
        assert_eq!(window.find("help-shortcuts-filter").focused(), Some(true));
        window.input("ALPHA action", cx);
        assert!(window.find("help-key-a-Alpha action").visible());
        assert!(window.try_find("help-key-b-Beta action").is_none());
        assert!(window.try_find("help-key-w-Wrap long titles").is_none());
        window.press("secondary-a", cx);
        window.press("backspace", cx);
        assert!(window.find("help-key-w-Wrap long titles").visible());
        window.input("no matching shortcut?", cx);
        assert!(window.find("help-shortcuts-empty").visible());
        assert!(
            app.read(cx).help_open,
            "a question mark in the filter must type, not close Help"
        );
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!app.read_with(cx, |app, _| app.help_open));
}

#[gpui_kit::test]
fn zoomed_help_does_not_offer_the_hidden_spaces_find_commands(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    assert!(app.update(cx, |app, cx| {
        app.shell_commands(cx)
            .iter()
            .any(|c| c.label == "Find in this Space")
    }));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("help", cx);
        window.click("zoom-in", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!app.update(cx, |app, cx| {
        app.shell_commands(cx)
            .iter()
            .any(|c| c.label == "Find in this Space")
    }));
}
