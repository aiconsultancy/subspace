//! The collapsed toolbar keeps every global command reachable from one menu.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::super::super::SubspaceApp;
use super::status::rooted_fixture;
use crate::toolbar_labels;

const MENU_ROWS: &[&str] = &[
    "Chat",
    "Search",
    "Commands",
    "Reload",
    "Wrap",
    "Theme designer",
    "Settings",
    "Help",
    "About",
    #[cfg(feature = "report-issue")]
    "Report issue",
];

const CONTROL_IDS: &[&str] = &[
    "toggle-chat",
    "open-full-text-search",
    "open-palette",
    "reload",
    "wrap",
    "jig",
    "settings",
    "help",
    "about",
    #[cfg(feature = "report-issue")]
    "report-issue",
];

fn collapse(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update(|cx| toolbar_labels::set_collapsed(true, cx));
    for _ in 0..4 {
        cx.update_window(window, |_, window, cx| {
            window.refresh();
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
}

fn collapsed_fixture(
    cx: &mut TestAppContext,
    root: &std::path::Path,
) -> (AnyWindowHandle, Entity<SubspaceApp>) {
    let (window, app) = rooted_fixture(cx, root, 2200., true);
    collapse(window, cx);
    (window, app)
}

#[gpui_kit::test]
fn the_toolbar_shows_its_controls_until_collapsed(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = rooted_fixture(cx, root.path(), 2200., false);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let toolbar = window.within("global-toolbar");
        for &id in CONTROL_IDS {
            assert!(toolbar.try_find(id).is_some(), "{id} starts inline");
        }
        assert!(toolbar.try_find("toolbar-menu").is_none());
    })
    .unwrap();
    collapse(window, cx);
    cx.update_window(window, |_, window, _| {
        let toolbar = window.within("global-toolbar");
        for &id in CONTROL_IDS {
            assert!(toolbar.try_find(id).is_none(), "{id} moves into the menu");
        }
        assert_eq!(toolbar.find("toolbar-menu").label(), Some("Toolbar menu"));
        assert_eq!(
            window.try_find("status-report-issue").is_some(),
            cfg!(feature = "report-issue")
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_toolbar_menu_lists_each_control_in_order_and_runs_its_command(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = collapsed_fixture(cx, root.path());
    cx.update_window(window, |_, window, cx| {
        window.click("toolbar-menu", cx);
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        let rows: Vec<_> = (0..MENU_ROWS.len())
            .map(|index| menu.find(index).label().map(str::to_owned))
            .collect();
        let expected: Vec<_> = MENU_ROWS
            .iter()
            .map(|label| Some((*label).to_owned()))
            .collect();
        assert_eq!(rows, expected);
        let help = MENU_ROWS.iter().position(|row| *row == "Help").unwrap();
        menu.click(help, cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.help_open));
    cx.update_window(window, |_, window, cx| {
        window.click("toolbar-menu", cx);
        window.render_frame(cx);
        let help = MENU_ROWS.iter().position(|row| *row == "Help").unwrap();
        assert_eq!(window.within("popup-menu").find(help).checked(), Some(true));
    })
    .unwrap();
}
