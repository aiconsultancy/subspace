//! Adding a Space from the title bar's plus, the mirror of closing it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, Role, TestAppContext};
use subspace_dashboard::prefs;

use super::super::add_space::menu::TITLE_BAR_ID;
use super::super::add_space::{ALL_ON, MANAGE_SPACES, off_spaces};
use super::super::{ACTIVE_KEY, SubspaceApp};
use super::{
    CLOSE_SPACE, IDS, active, choose, ids, open, running_config, saved_boards, settings_save,
    settle,
};

fn off(cx: &mut TestAppContext) -> Vec<&'static str> {
    cx.read(|cx| off_spaces(cx).into_iter().map(|space| space.id).collect())
}

fn add(window: AnyWindowHandle, app: &Entity<SubspaceApp>, id: &str, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.open_space(id, window, cx));
    })
    .unwrap();
    settle(window, cx);
}

fn last_toast(app: &Entity<SubspaceApp>, cx: &mut TestAppContext) -> String {
    app.read_with(cx, |app, _| {
        app.toasts.last().unwrap().toast.text.to_string()
    })
}

/// Clicks the plus, which opens its menu.
fn open_plus(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.click(TITLE_BAR_ID, cx))
        .unwrap();
    settle(window, cx);
}

/// The element id of Space `id`'s title in the plus menu.
fn space_title(id: &str) -> ElementId {
    ElementId::Name(format!("add-space-item-{id}").into())
}

/// Clicks the plus and chooses the menu line labelled `label`.
fn choose_added(window: AnyWindowHandle, label: &str, cx: &mut TestAppContext) {
    open_plus(window, cx);
    let item = cx
        .update_window(window, |_, window, _| {
            window.within("popup-menu").find_by_label(label)
        })
        .unwrap();
    let target = item.path().last().cloned().unwrap();
    cx.update_window(window, |_, window, cx| {
        window.within("popup-menu").click(target, cx);
    })
    .unwrap();
    settle(window, cx);
}

#[gpui_kit::test]
fn adding_a_closed_space_brings_its_tab_back_and_switches_to_it(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    assert!(off(cx).is_empty());
    choose(window, "beta", CLOSE_SPACE, cx);
    assert_eq!(off(cx), ["beta"]);

    add(window, &app, "beta", cx);
    assert_eq!(ids(&app, cx), IDS);
    assert_eq!(active(&app, cx), "beta");
    assert!(off(cx).is_empty());
    assert!(running_config(cx).board_enabled("beta"));
    assert_eq!(
        saved_boards(root.path()),
        Some(serde_json::json!({"gamma": true}))
    );
    assert_eq!(last_toast(&app, cx), "Opened Beta.");
}

/// When the board set cannot be rebuilt, the Space's tab waits for a
/// restart, and the restart opens on it as choosing it now would have.
#[gpui_kit::test]
fn a_space_added_for_after_a_restart_is_current_after_the_restart(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    choose(window, "beta", CLOSE_SPACE, cx);
    cx.update(crate::registry::forget_provider);

    add(window, &app, "beta", cx);
    assert_eq!(ids(&app, cx), ["alpha", "gamma"]);
    assert_eq!(active(&app, cx), "alpha");
    assert_eq!(last_toast(&app, cx), "Restart the app to open Beta.");
    assert_eq!(
        saved_boards(root.path()),
        Some(serde_json::json!({"gamma": true}))
    );
    let startup = cx.read(|cx| prefs::get(cx, ACTIVE_KEY));
    assert_eq!(startup.as_deref(), Some("beta"));
}

#[gpui_kit::test]
fn adding_saves_what_the_settings_switch_saves(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    // Gamma is off by default, so adding it writes an explicit `true`.
    for id in ["gamma", "beta"] {
        choose(window, id, CLOSE_SPACE, cx);
        add(window, &app, id, cx);
        let from_plus = (saved_boards(root.path()), running_config(cx).boards);
        choose(window, id, CLOSE_SPACE, cx);
        settings_save(window, &app, id, true, cx);
        let from_settings = (saved_boards(root.path()), running_config(cx).boards);
        assert_eq!(from_plus, from_settings, "{id}");
        assert!(ids(&app, cx).contains(&id));
    }
}

#[gpui_kit::test]
fn a_space_that_is_already_on_is_not_added_again(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    let saved = saved_boards(root.path());
    let toasts = app.read_with(cx, |app, _| app.toasts.len());
    add(window, &app, "alpha", cx);
    assert_eq!(ids(&app, cx), IDS);
    assert_eq!(saved_boards(root.path()), saved);
    assert_eq!(app.read_with(cx, |app, _| app.toasts.len()), toasts);
}

#[gpui_kit::test]
fn the_plus_menu_says_when_every_space_is_on(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _app) = open(root.path(), cx);
    cx.update_window(window, |_, window, cx| window.click(TITLE_BAR_ID, cx))
        .unwrap();
    settle(window, cx);
    cx.update_window(window, |_, window, _| {
        let menu = window.within("popup-menu");
        assert!(menu.try_find_by_label(ALL_ON).is_some());
        assert!(menu.try_find_by_label(MANAGE_SPACES).is_some());
    })
    .unwrap();
}

/// A Space's line is a menu item that takes its accessible name from the
/// accessible label inside it, which holds the Space's title.
#[gpui_kit::test]
fn each_space_in_the_plus_menu_is_named_by_its_title(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _app) = open(root.path(), cx);
    choose(window, "gamma", CLOSE_SPACE, cx);
    open_plus(window, cx);
    cx.update_window(window, |_, window, _| {
        let menu = window.within("popup-menu");
        let title = menu.find(space_title("gamma"));
        assert_eq!(title.role(), Some(Role::Label));
        assert_eq!(title.value(), Some("Gamma"));
        let items = menu.find_all_by_role(Role::MenuItem);
        assert!(
            items
                .iter()
                .any(|item| title.path().starts_with(item.path())),
            "the title is inside its menu item"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn choosing_a_space_in_the_plus_menu_adds_it(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    choose(window, "gamma", CLOSE_SPACE, cx);
    open_plus(window, cx);
    cx.update_window(window, |_, window, cx| {
        window.within("popup-menu").click(space_title("gamma"), cx);
    })
    .unwrap();
    settle(window, cx);
    assert_eq!(ids(&app, cx), IDS);
    assert_eq!(active(&app, cx), "gamma");
    assert_eq!(last_toast(&app, cx), "Opened Gamma.");
}

#[gpui_kit::test]
fn manage_spaces_opens_settings_at_the_first_space_that_is_off(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    choose(window, "gamma", CLOSE_SPACE, cx);
    choose_added(window, MANAGE_SPACES, cx);
    let settings = app.read_with(cx, |app, _| app.settings.clone()).unwrap();
    assert!(settings.read_with(cx, |settings, _| settings.shows_board_page("gamma")));
}
