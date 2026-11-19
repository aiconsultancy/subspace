use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle};
use subspace_dashboard::prefs;

use super::{Settings, click, open_settings, press};
use crate::app::navigation::{COLLAPSED_KEY, NAVIGATION_KEY};
use crate::settings::{CHECK_UPDATES_KEY, CONFIRM_QUIT_KEY};
use crate::toolbar_labels::ToolbarControl;

fn select_previous(settings: WindowHandle<Settings>, id: &'static str, cx: &mut TestAppContext) {
    cx.update_window(settings.into(), |_, window, cx| {
        window.within(id).click("input", cx);
    })
    .unwrap();
    cx.run_until_parked();
    press(settings, "up", cx);
    press(settings, "enter", cx);
}

fn edit_drafts(settings: WindowHandle<Settings>, cx: &mut TestAppContext) {
    click(settings, "settings-app:layout", cx);
    click(settings, "board-order-down-future", cx);
    click(settings, "collapsed-board-navigation", cx);
    click(settings, ("toolbar-label", 0_usize), cx);
    select_previous(settings, "board-navigation-mode", cx);
    click(settings, "settings-app:general", cx);
    click(settings, "automatic-board-refresh", cx);
    click(settings, "check-updates-at-startup", cx);
    click(settings, "confirm-before-quitting", cx);
}

#[gpui_kit::test]
fn layout_and_general_ui_drafts_save_to_the_existing_settings_keys(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    edit_drafts(settings, cx);
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(prefs::get(cx, NAVIGATION_KEY), None);
            assert_eq!(prefs::get(cx, CHECK_UPDATES_KEY), None);
            assert!(
                !settings.save(window, cx).unwrap(),
                "board order and layout preferences apply without restart"
            );
        })
        .unwrap();
    let config = subspace_core::config::load(&root.path().join("config.json")).0;
    assert_eq!(config.board_order, ["other", "future"]);
    assert!(config.terminal.is_empty());
    assert!(!config.auto_refresh);
    cx.update(|cx| {
        prefs::init_file(Some(root.path().join("state.json")), cx);
        assert_eq!(prefs::get(cx, NAVIGATION_KEY).as_deref(), Some("vertical"));
        assert_eq!(prefs::get(cx, COLLAPSED_KEY).as_deref(), Some("1"));
        assert_eq!(prefs::get(cx, CHECK_UPDATES_KEY).as_deref(), Some("0"));
        assert_eq!(prefs::get(cx, CONFIRM_QUIT_KEY).as_deref(), Some("true"));
        assert!(ToolbarControl::Chat.shows_label(cx));
    });
    let reopened = open_settings(root.path(), cx);
    reopened
        .update(cx, |settings, _, _| {
            assert_eq!(
                settings
                    .boards
                    .iter()
                    .map(|board| board.id)
                    .collect::<Vec<_>>(),
                ["other", "future"]
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn cancelling_layout_and_general_ui_drafts_writes_neither_file(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    let state = std::fs::read(root.path().join("state.json")).unwrap();
    let config = std::fs::read(root.path().join("config.json")).unwrap();
    edit_drafts(settings, cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.board_navigation.collapsed);
            assert!(!settings.check_updates());
            settings.restore_preview(window, cx).unwrap();
        })
        .unwrap();
    assert_eq!(
        std::fs::read(root.path().join("state.json")).unwrap(),
        state
    );
    assert_eq!(
        std::fs::read(root.path().join("config.json")).unwrap(),
        config
    );
}
