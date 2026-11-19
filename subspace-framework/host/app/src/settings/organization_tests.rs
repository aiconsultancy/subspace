#[cfg(debug_assertions)]
mod developer;
mod drag;
mod persistence;

use std::path::Path;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle, point, px, size};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_dashboard::prefs;

use super::{Settings, SettingsPage};
use crate::theme::{self, THEME_PREF};

pub(super) fn open_settings(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        prefs::set(cx, THEME_PREF, "Solarized Light");
        theme::initialize(cx);
        let mut future = super::navigation_tests::registration();
        future.settings.notifications = true;
        future.settings.background_updates = true;
        let other = crate::registry::BoardRegistration {
            id: "other",
            ..future.clone()
        };
        // Identical titles ensure routes and controls use registered IDs.
        cx.set_global(crate::registry::IncludedBoards(vec![future, other]));
        prefs::save(cx).unwrap();
    });
    let path = root.join("config.json");
    if !path.exists() {
        std::fs::write(&path, "{}").unwrap();
    }
    let file = SettingsFile::open(&path).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    cx.simulate_window_resize(settings.into(), size(px(900.), px(1500.)));
    settings
}

fn click(
    settings: WindowHandle<Settings>,
    id: impl Into<gpui_kit::ElementId>,
    cx: &mut TestAppContext,
) {
    let id = id.into();
    cx.update_window(settings.into(), |_, window, cx| window.click(id, cx))
        .unwrap();
    cx.run_until_parked();
}

fn press(settings: WindowHandle<Settings>, key: &str, cx: &mut TestAppContext) {
    cx.update_window(settings.into(), |_, window, cx| window.press(key, cx))
        .unwrap();
    cx.run_until_parked();
}

fn assert_navigation_focused(
    settings: WindowHandle<Settings>,
    stage: &str,
    cx: &mut TestAppContext,
) {
    let id = settings
        .read_with(cx, |settings, _| {
            format!("tree-{}", settings.navigation.state.entity_id())
        })
        .unwrap();
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(id).focused(),
            Some(true),
            "the native app navigation tree must retain keyboard focus {stage}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn layout_controls_have_one_destination_and_general_keeps_startup_behavior(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("check-updates-at-startup").visible());
        assert!(window.find("automatic-board-refresh").visible());
        let reveal = window.find("settings-reveal-config");
        assert!(reveal.visible());
        assert_eq!(reveal.label(), Some(subspace_dashboard::reveal::label()));
        assert!(window.try_find("board-order-down-future").is_none());
    })
    .unwrap();
    click(settings, "settings-app:layout", cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("board-order-down-future").visible());
        assert!(window.find("collapsed-board-navigation").visible());
        assert!(
            window.try_find("terminal-placement").is_none(),
            "no terminal choices without a terminal"
        );
        assert!(window.try_find("check-updates-at-startup").is_none());
    })
    .unwrap();
    click(settings, "settings-app:appearance", cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("collapsed-board-navigation").is_none());
        assert!(window.try_find("board-order-down-future").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn reordering_then_clicking_registered_board_and_section_keeps_their_identity(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    click(settings, "settings-app:layout", cx);
    click(settings, "board-order-down-future", cx);
    click(settings, "settings-board:future", cx);
    settings
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
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("settings-future-enabled").visible());
        assert!(window.try_find("settings-other-enabled").is_none());
    })
    .unwrap();
    click(settings, "settings-board:future/nested", cx);
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(
                settings.page,
                SettingsPage::BoardSection {
                    board: "future",
                    path: vec![0]
                }
            );
            settings.select_initial_board("other", cx);
            assert_eq!(
                settings.navigation.selected_page(cx),
                Some(SettingsPage::Board("other"))
            );
            assert!(
                settings
                    .navigation
                    .board_state
                    .read(cx)
                    .index_of(&"board:future/nested".into())
                    .is_some()
            );
        })
        .unwrap();
    click(settings, "settings-board:future/nested", cx);
    assert_eq!(
        settings
            .read_with(cx, |settings, _| settings.page.clone())
            .unwrap(),
        SettingsPage::BoardSection {
            board: "future",
            path: vec![0]
        }
    );
}

#[gpui_kit::test]
fn boards_heading_is_not_a_destination_and_arrows_cross_both_ways(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    let (last, last_page) = super::view::navigation::last_app_item();
    click(settings, last, cx);
    assert_navigation_focused(settings, "after selecting a page", cx);
    click(settings, "settings-boards-heading", cx);
    assert_navigation_focused(settings, "after clicking the heading", cx);
    click(settings, "settings-app-heading", cx);
    assert_navigation_focused(settings, "after clicking the application heading", cx);
    assert_eq!(
        settings
            .read_with(cx, |settings, _| settings.page.clone())
            .unwrap(),
        last_page
    );
    press(settings, "down", cx);
    assert_eq!(
        settings
            .read_with(cx, |settings, _| settings.page.clone())
            .unwrap(),
        SettingsPage::Board("future")
    );
    press(settings, "up", cx);
    assert_eq!(
        settings
            .read_with(cx, |settings, _| settings.page.clone())
            .unwrap(),
        last_page
    );
    press(settings, "down", cx);
    press(settings, "right", cx);
    press(settings, "down", cx);
    assert_eq!(
        settings
            .read_with(cx, |settings, _| settings.page.clone())
            .unwrap(),
        SettingsPage::BoardSection {
            board: "future",
            path: vec![0]
        }
    );
}

#[gpui_kit::test]
fn narrow_settings_content_scrolls_without_moving_navigation(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    // Select Layout first: the short window's application tree scrolls.
    click(settings, "settings-app:layout", cx);
    cx.simulate_window_resize(settings.into(), size(px(640.), px(620.)));
    cx.update_window(settings.into(), |_, window, cx| {
        window.set_rem_size(px(20.));
        window.render_frame(cx);
        let navigation = window.find("settings-navigation").bounds();
        let content = window.find("settings-scroll").bounds();
        let general = window.find("settings-app:general").bounds();
        assert!(navigation.right() <= content.left());
        assert!(navigation.top() < content.top());
        assert!(navigation.bottom() > content.bottom());
        window.scroll(
            "settings-scroll",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-600.))),
            cx,
        );
        assert_eq!(window.find("settings-app:general").bounds(), general);
        assert!(window.find("settings-boards-heading").visible());
    })
    .unwrap();
    settings
        .update(cx, |settings, _, _| {
            assert!(settings.scroll.offset().y < px(0.));
            assert_eq!(settings.page, SettingsPage::Layout);
        })
        .unwrap();
}
