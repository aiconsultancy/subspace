use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle};
use subspace_core::state::State;
use subspace_dashboard::prefs;

use super::Settings;
use super::preview_tests::open_settings;
use crate::app::navigation::{COLLAPSED_KEY, NAVIGATION_KEY};
use crate::toolbar_labels::{self, ToolbarControl};

#[gpui_kit::test]
fn saved_toolbar_labels_survive_reloading_preferences(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.set_toolbar_label(3, true, cx);
            assert!(!ToolbarControl::Commands.shows_label(cx));
            assert!(!settings.save(window, cx).unwrap(), "no restart needed");
        })
        .unwrap();
    cx.update(|cx| {
        prefs::init_file(Some(root.path().join("state.json")), cx);
        assert!(ToolbarControl::Commands.shows_label(cx));
        assert!(!ToolbarControl::Search.shows_label(cx));
    });
    let reopened = open_settings(root.path(), cx);
    reopened
        .update(cx, |settings, _, _| {
            assert!(settings.toolbar.labels[3]);
            assert!(!settings.toolbar.labels[2]);
        })
        .unwrap();
}

#[gpui_kit::test]
fn cancelled_toolbar_drafts_do_not_change_preferences(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.set_toolbar_label(0, true, cx);
            settings.restore_preview(window, cx).unwrap();
            assert!(!ToolbarControl::Chat.shows_label(cx));
        })
        .unwrap();
    let state = State::load(&root.path().join("state.json"));
    assert_eq!(state.get(ToolbarControl::Chat.key()), None);
}

#[gpui_kit::test]
fn saving_an_untouched_draft_keeps_context_menu_changes(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            ToolbarControl::Help.set_label(true, cx);
            settings.set_toolbar_label(3, true, cx);
            settings.save(window, cx).unwrap();
            assert!(ToolbarControl::Commands.shows_label(cx));
            assert!(ToolbarControl::Help.shows_label(cx));
        })
        .unwrap();
    let state = State::load(&root.path().join("state.json"));
    assert_eq!(state.get(ToolbarControl::Help.key()), Some("1"));
}

fn label_switches_disabled(settings: WindowHandle<Settings>, cx: &mut TestAppContext) -> Vec<bool> {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        (0..ToolbarControl::ALL.len())
            .filter(|index| ToolbarControl::ALL[*index].offered(false, cx))
            .map(|index| window.find(("toolbar-label", index)).disabled() == Some(true))
            .collect()
    })
    .unwrap()
}

#[gpui_kit::test]
fn collapsing_the_toolbar_disables_label_switches_and_saves_on_save(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = super::organization_tests::open_settings(root.path(), cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("settings-app:layout", cx)
    })
    .unwrap();
    cx.run_until_parked();
    assert!(label_switches_disabled(settings, cx).iter().all(|off| !off));
    cx.update_window(settings.into(), |_, window, cx| {
        assert_eq!(window.find("collapse-toolbar").checked(), Some(false));
        window.click("collapse-toolbar", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(label_switches_disabled(settings, cx).iter().all(|off| *off));
    settings
        .update(cx, |settings, window, cx| {
            assert!(!toolbar_labels::collapsed(cx), "Save applies the draft");
            assert!(!settings.save(window, cx).unwrap(), "no restart needed");
        })
        .unwrap();
    cx.update(|cx| {
        prefs::init_file(Some(root.path().join("state.json")), cx);
        assert!(toolbar_labels::collapsed(cx));
    });
    let reopened = super::organization_tests::open_settings(root.path(), cx);
    reopened
        .update(cx, |settings, _, _| assert!(settings.toolbar.menu))
        .unwrap();
}

#[gpui_kit::test]
fn cancelled_toolbar_menu_draft_does_not_change_preferences(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.set_toolbar_menu(true, cx);
            settings.restore_preview(window, cx).unwrap();
            assert!(!toolbar_labels::collapsed(cx));
        })
        .unwrap();
    let state = State::load(&root.path().join("state.json"));
    assert_eq!(state.get("app.toolbar-menu"), None);
}

#[gpui_kit::test]
fn layout_navigation_save_and_reopen_preserves_the_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.board_navigation.mode.update(cx, |select, cx| {
                select.set_selected_value(&"Vertical tabs".to_owned(), window, cx);
            });
            settings.board_navigation.collapsed = true;
            settings.save(window, cx).unwrap();
        })
        .unwrap();
    cx.update(|cx| {
        assert_eq!(prefs::get(cx, NAVIGATION_KEY).as_deref(), Some("vertical"));
        assert_eq!(prefs::get(cx, COLLAPSED_KEY).as_deref(), Some("1"));
    });
    let reopened = open_settings(root.path(), cx);
    reopened
        .update(cx, |settings, _, cx| {
            assert_eq!(
                settings.board_navigation.mode.read(cx).selected_value(),
                Some(&"Vertical tabs".to_owned())
            );
            assert!(settings.board_navigation.collapsed);
        })
        .unwrap();
}

#[gpui_kit::test]
fn cancelled_navigation_draft_does_not_change_preferences(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.board_navigation.mode.update(cx, |select, cx| {
                select.set_selected_value(&"Vertical tabs".to_owned(), window, cx);
            });
            settings.board_navigation.collapsed = true;
            settings.restore_preview(window, cx).unwrap();
        })
        .unwrap();
    cx.update(|cx| {
        assert_eq!(prefs::get(cx, NAVIGATION_KEY), None);
        assert_eq!(prefs::get(cx, COLLAPSED_KEY), None);
    });
}

#[gpui_kit::test]
fn untouched_navigation_draft_preserves_palette_changes(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    cx.update(|cx| {
        prefs::set(cx, NAVIGATION_KEY, "vertical");
        prefs::set(cx, COLLAPSED_KEY, "1");
    });
    settings
        .update(cx, |settings, window, cx| {
            settings.save(window, cx).unwrap();
        })
        .unwrap();
    let state = State::load(&root.path().join("state.json"));
    assert_eq!(state.get(NAVIGATION_KEY), Some("vertical"));
    assert_eq!(state.get(COLLAPSED_KEY), Some("1"));
}
