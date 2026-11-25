use super::client;
use crate::{Account, Github, SectionData, sample};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Context, Entity, TestAppContext, VisualTestContext, Window};
use std::sync::Arc;
use subspace_framework::client_github::model::{GhItemDetail, PrState};
use subspace_framework::client_test_support::Mock;
use subspace_framework::dashboard::{Dashboard as _, Modal};

mod availability;
mod merge;
mod success;

fn detail() -> GhItemDetail {
    GhItemDetail {
        item: sample::items().remove(0),
        body_html: String::new(),
        reviewers: Vec::new(),
        check_runs: Vec::new(),
        events: Vec::new(),
        viewer_can_review: true,
        viewer_can_comment: true,
        changed_files: Some(0),
        merge_target: None,
        viewer_approved: false,
    }
}

fn board(window: &mut Window, cx: &mut Context<Github>) -> Github {
    let mut board = Github::new(window, cx);
    board.client = None;
    board.started = true;
    board.account = Some("ann".into());
    board.accounts = vec!["ann".into()];
    let detail = detail();
    let key = detail.item.key.clone();
    let mut account = Account::default();
    account.sections.insert(
        board.section,
        SectionData {
            total: 1,
            at: 0,
            items: vec![detail.item.clone()],
        },
    );
    account.details.insert(key.clone(), detail);
    board.data.insert("ann".into(), account);
    board.rebuild(cx);
    board.list.reveal(&key);
    board.focus.focus(window, cx);
    board
}

fn setup(cx: &mut TestAppContext) -> (Entity<Github>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_framework::dashboard::prefs::init_file(None, cx);
        subspace_framework::dashboard::bind_keys(cx);
        Github::bind_keys(cx);
    });
    let (board, cx) = cx.add_window_view(board);
    settle(cx);
    (board, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn shortcut_opens_modal_escape_cancels_and_returns_focus(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx);
    cx.simulate_keystrokes("x");
    settle(cx);
    board.read_with(cx, |board, cx| {
        assert_eq!(board.modal(cx), Some(Modal::Dialog))
    });
    cx.update(|window, cx| window.input("Optional\ncomment", cx));
    cx.simulate_keystrokes("escape");
    settle(cx);
    board.read_with(cx, |board, cx| assert_eq!(board.modal(cx), None));
    board.update_in(cx, |board, window, _| {
        assert!(board.focus.is_focused(window))
    });
}

#[gpui_kit::test]
fn empty_comment_can_confirm_and_validation_keeps_text(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx);
    cx.simulate_keystrokes("x");
    settle(cx);
    cx.update(|window, _| assert_ne!(window.find("github-close-submit").disabled(), Some(true)));
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(
            board
                .close_confirmation
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("unavailable")
        )
    });
    board.update_in(cx, |board, window, cx| {
        board
            .close_confirmation
            .as_ref()
            .unwrap()
            .comment
            .update(cx, |input, cx| {
                input.set_value("x".repeat(65537), window, cx)
            });
    });
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    settle(cx);
    board.read_with(cx, |board, cx| {
        let pending = board.close_confirmation.as_ref().unwrap();
        assert_eq!(
            pending.error.as_deref(),
            Some("Keep the comment within 64 KB.")
        );
        assert_eq!(pending.comment.read(cx).value().len(), 65537);
        assert!(!board.writing);
    });
    cx.update(|window, cx| window.click("github-close-cancel", cx));
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.close_confirmation.is_none()));
}

#[gpui_kit::test]
fn failed_comment_keeps_the_dialog_and_draft_via_the_submit_button(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (403, "{}".into()));
    let (board, cx) = setup(cx);
    board.update(cx, |board, _| board.client = Some(Arc::new(client(&mock))));
    cx.simulate_keystrokes("x");
    settle(cx);
    cx.update(|window, cx| {
        window.input("Reason to close\nSecond line", cx);
        window.click("github-close-submit", cx);
    });
    settle(cx);
    board.read_with(cx, |board, cx| {
        let pending = board.close_confirmation.as_ref().unwrap();
        assert_eq!(
            pending.comment.read(cx).value().as_ref(),
            "Reason to close\nSecond line"
        );
        assert!(
            pending
                .error
                .as_ref()
                .unwrap()
                .contains("comment was not posted")
        );
        assert!(!pending.posted);
        assert!(!board.writing);
    });
    assert_eq!(mock.requests().len(), 1);
}

#[gpui_kit::test]
fn failed_close_keeps_posted_text_and_keyboard_retry_only_closes(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.method == "PATCH" {
            (422, "{}".into())
        } else {
            (201, "{}".into())
        }
    });
    let (board, cx) = setup(cx);
    board.update(cx, |board, _| board.client = Some(Arc::new(client(&mock))));
    cx.simulate_keystrokes("x");
    settle(cx);
    cx.update(|window, cx| window.input("Reason", cx));
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    settle(cx);
    board.read_with(cx, |board, cx| {
        let pending = board.close_confirmation.as_ref().unwrap();
        assert!(pending.posted);
        assert!(pending.error.as_ref().unwrap().contains("still open"));
        assert_eq!(pending.comment.read(cx).value().as_ref(), "Reason");
    });
    assert_eq!(mock.requests().len(), 2);
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    settle(cx);
    let methods: Vec<_> = mock.requests().into_iter().map(|r| r.method).collect();
    assert_eq!(methods, ["POST", "PATCH", "PATCH"]);
}

#[gpui_kit::test]
fn successful_close_updates_the_panel_and_row_even_if_refresh_fails(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.path == "/graphql" {
            (500, "{}".into())
        } else {
            (200, "{}".into())
        }
    });
    let (board, cx) = setup(cx);
    board.update(cx, |board, _| board.client = Some(Arc::new(client(&mock))));
    cx.simulate_keystrokes("x");
    settle(cx);
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(board.close_confirmation.is_none());
        let account = &board.data["ann"];
        assert_eq!(
            account.sections[board.section].items[0].state,
            PrState::Closed
        );
        assert_eq!(
            account.details.values().next().unwrap().item.state,
            PrState::Closed
        );
        assert_eq!(board.selected_item().unwrap().state, PrState::Closed);
    });
    let requests = mock.requests();
    assert_eq!(requests[0].method, "PATCH");
    assert_eq!(requests[1].path, "/graphql");
    assert_eq!(
        requests.len(),
        2,
        "a refresh failure must not retry the write"
    );
}
