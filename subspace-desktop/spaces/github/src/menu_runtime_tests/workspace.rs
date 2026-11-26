//! The workspace around the board: the open pull request's page, the
//! inspector beside it, and the navigator's repositories.

use super::{settle, setup, status, use_dock_switch};
use crate::{Account, Github, MenuTarget, SectionData, sample};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Context, TestAppContext};
use std::sync::Arc;
use subspace_framework::client_github::{
    Client,
    model::{GhItemDetail, GhUser, ItemKind},
};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};

#[gpui_kit::test]
fn the_page_sits_beside_the_inspector_and_keys_reach_the_space_from_it(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    board.update(cx, load_detail);
    settle(cx);
    cx.update(|window, _| {
        let strip = window.find("workspace-tab-strip").bounds();
        let toolbar = window.find("github-page-toolbar").bounds();
        let inspector = window.find("github-inspector").bounds();
        assert!(window.try_find("github-page").is_some());
        assert_eq!(
            inspector.top(),
            strip.top(),
            "the inspector's header is level"
        );
        assert!(toolbar.right() <= inspector.left());
        assert!(window.try_find("gh-page-back").is_some());
        let approve = window.find(("gh-approve", 4usize)).bounds();
        let files = window.find(("gh-diff", 4usize)).bounds();
        let close = window.find(("gh-close", 4usize)).bounds();
        assert!(approve.right() <= files.left());
        assert!(files.right() <= close.left());
        assert!(window.try_find(("gh-comment", 4usize)).is_none());
        assert!(window.try_find(("gh-changes", 4usize)).is_none());
        assert!(window.try_find(("gh-approve", 3usize)).is_some(), "footer");
        assert!(
            window.try_find("gh-peek-open").is_none(),
            "the page is open"
        );
    });
    // A key reaches the Space while focus is in the inspector: the menu
    // returns the focus to its footer button, then `x` asks to close.
    let id = ("gh-assign", 3usize);
    cx.update(|window, cx| window.click(id, cx));
    settle(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, _| assert_eq!(window.find(id).focused(), Some(true)));
    cx.simulate_keystrokes("x");
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.close_confirmation.is_some()));
    cx.simulate_keystrokes("escape");
    settle(cx);
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("escape");
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(board.list.open_row().is_none(), "Escape goes back");
        assert!(board.list.selected_row().is_some());
    });
    cx.update(|window, _| {
        assert!(window.try_find("github-page").is_none());
        assert!(window.try_find("gh-peek-open").is_some());
    });
}

#[gpui_kit::test]
fn review_composer_fits_the_inspector_and_keeps_its_width_when_sending(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1100.);
    board.update(cx, load_detail);
    settle(cx);
    cx.update(|window, cx| window.click(("gh-approve", 3usize), cx));
    settle(cx);
    let width = cx.update(|window, _| {
        let inspector = window.find("github-inspector").bounds();
        let composer = window.find("composer").bounds();
        let submit = window.find("composer-submit").bounds();
        assert!(composer.right() < inspector.right());
        assert!(submit.right() <= composer.right());
        composer.size.width
    });
    board.update(cx, |board, cx| {
        board
            .composer
            .as_ref()
            .unwrap()
            .entity
            .update(cx, |composer, cx| {
                composer.set_busy(true, cx);
            });
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(window.find("composer").bounds().size.width, width);
    });
}

#[gpui_kit::test]
fn one_click_selects_and_fills_the_inspector_without_opening_the_page(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    board.update(cx, load_sample_view);
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("gh-peek-empty").is_some()));
    cx.update(|window, cx| window.click("gh-row:octo-org/api#421", cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        let selected = board.list.selected().map(|key| key.to_string());
        assert_eq!(selected.as_deref(), Some("octo-org/api#421"));
        assert!(board.list.open_row().is_none(), "one click only selects");
    });
    cx.update(|window, _| {
        assert!(window.try_find("github-page").is_none());
        let title = window.find("gh-peek-title");
        assert_eq!(title.label(), Some("Add paging to the work items endpoint"));
        assert_eq!(window.find("gh-peek-state").label(), Some("Open"));
    });
    cx.update(|window, cx| window.click("gh-peek-open", cx));
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.list.open_row().is_some()));
    cx.update(|window, _| assert!(window.try_find("github-page").is_some()));
}

#[gpui_kit::test]
fn enter_and_a_double_click_open_the_page(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    board.update(cx, load_sample_view);
    board.update(cx, |board, _| board.list.select("octo-org/api#421".into()));
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    settle(cx);
    cx.simulate_keystrokes("enter");
    settle(cx);
    let open = |board: &Github| board.list.open_row().map(|row| row.key.to_string());
    board.read_with(cx, |board, _| {
        assert_eq!(open(board).as_deref(), Some("octo-org/api#421"));
    });
    cx.simulate_keystrokes("escape");
    settle(cx);
    board.read_with(cx, |board, _| assert_eq!(open(board), None));
    cx.update(|window, cx| window.double_click("gh-row:octo-org/web#1203", cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(open(board).as_deref(), Some("octo-org/web#1203"));
    });
    cx.update(|window, _| assert!(window.try_find("github-page").is_some()));
}

#[gpui_kit::test]
fn the_inspector_switch_and_close_hide_and_show_the_inspector(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    cx.update(|window, cx| window.click("workspace-inspector-close", cx));
    settle(cx);
    board.read_with(cx, |board, cx| {
        assert!(!board.inspector_open);
        let stored = subspace_framework::dashboard::prefs::get(cx, crate::layout::INSPECTOR_KEY);
        assert_eq!(stored.as_deref(), Some("0"));
    });
    cx.update(|window, _| assert!(window.try_find("github-inspector").is_none()));
    let docks = board.read_with(cx, |board, cx| board.docks(cx));
    assert_eq!(docks, SpaceDocks::both(true, false));
    use_dock_switch(&board, cx, SpaceDock::Inspector);
    board.read_with(cx, |board, _| assert!(board.inspector_open));
    cx.update(|window, _| assert!(window.try_find("gh-peek-empty").is_some()));
}

fn load_detail(board: &mut Github, cx: &mut Context<Github>) {
    let item = sample::items().remove(0);
    let key = item.key.clone();
    let detail = GhItemDetail {
        item: item.clone(),
        body_html: String::new(),
        reviewers: Vec::new(),
        check_runs: Vec::new(),
        events: Vec::new(),
        viewer_can_review: true,
        viewer_can_comment: true,
        changed_files: Some(0),
        merge_target: None,
        viewer_approved: false,
    };
    let mut account = Account::default();
    account.sections.insert(
        board.section,
        SectionData {
            total: 1,
            items: vec![item.clone()],
            at: 0,
        },
    );
    account.details.insert(key.clone(), detail);
    account.assignable.insert(
        item.repo.to_lowercase(),
        vec![GhUser {
            login: "test-assignee".into(),
            name: None,
        }],
    );
    board.data.insert("test-alpha".into(), account);
    let config = subspace_framework::core::config::Config::default();
    board.client = Some(Arc::new(
        Client::new(subspace_framework::agent_tools::test_support::credentials(
            &config,
        ))
        .with_api_url("http://127.0.0.1:0"),
    ));
    board.rebuild(cx);
    board.list.reveal(&key);
    cx.notify();
}

#[gpui_kit::test]
fn the_footer_assign_button_and_the_key_open_the_assignee_menu(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx, 1600.);
    board.update(cx, load_detail);
    settle(cx);
    let id = ("gh-assign", 3usize);
    cx.update(|window, cx| window.click(id, cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(matches!(
            board.menu.as_ref().map(|(_, target, _)| target),
            Some(MenuTarget::Assignees(_))
        ))
    });
    cx.update(|window, _| assert!(window.try_find("menu").is_some()));
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, _| assert_eq!(window.find(id).focused(), Some(true)));
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("a");
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("menu").is_some()));
    board.update_in(cx, |board, window, cx| board.deactivate(window, cx));
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.menu.is_none()));
}

/// The sample pull requests as the current view, in three repositories.
fn load_sample_view(board: &mut Github, cx: &mut Context<Github>) {
    let mut items = sample::items();
    items.retain(|item| item.kind == ItemKind::Pr);
    let mut account = Account::default();
    account.sections.insert(
        board.section,
        SectionData {
            total: items.len() as u32,
            items,
            at: 0,
        },
    );
    board.data.insert("test-alpha".into(), account);
    board.rebuild(cx);
}

#[gpui_kit::test]
fn a_navigator_repository_row_filters_the_list_and_a_second_click_clears_it(
    cx: &mut TestAppContext,
) {
    let (board, cx) = setup(cx, 1600.);
    board.update(cx, load_sample_view);
    settle(cx);
    let id = "gh-repo:octo-org/web";
    cx.update(|window, cx| {
        assert_eq!(window.find(id).selected(), Some(false));
        window.click(id, cx);
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.repo_filter.as_deref(), Some("octo-org/web"));
        let keys: Vec<&str> = board.list.groups().iter().map(|g| g.key.as_ref()).collect();
        assert_eq!(keys, ["repo:octo-org/web"]);
    });
    cx.update(|window, _| {
        assert_eq!(window.find(id).selected(), Some(true));
        let other = window.try_find("gh-repo:octo-org/api");
        assert!(other.is_some(), "the other repositories stay listed");
        assert!(window.try_find("gh-repo-filter-clear").is_some());
    });
    assert!(status(&board, cx).ends_with(" · octo-org/web only"));
    cx.update(|window, cx| window.click(id, cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(board.repo_filter.is_none());
        assert_eq!(board.list.groups().len(), 3);
    });
    cx.update(|window, _| assert!(window.try_find("gh-repo-filter-clear").is_none()));
}
