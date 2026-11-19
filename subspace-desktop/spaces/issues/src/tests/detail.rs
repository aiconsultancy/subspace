use std::sync::atomic::Ordering;

use gpui_kit::TestAppContext;
use subspace_api_client::hub::{IssueStatus, ReadError};
use subspace_framework::dashboard::Dashboard as _;

use super::authoring::open_first;
use super::offline::{TITLE, board_with_hub, offline, settle_board};
use super::{has, label, press, render, selected};
use crate::Issues;

fn is_loaded(board: &Issues, identifier: &str) -> bool {
    board.issue_view.snapshot.as_ref().is_some_and(|issue| {
        issue.identifier() == identifier && *issue.status() == IssueStatus::Loaded
    })
}

#[gpui_kit::test]
fn issue_detail_opens_with_enter_steps_and_returns_to_the_list(cx: &mut TestAppContext) {
    let offline = offline();
    let (board, cx) = board_with_hub(&offline, cx);
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    render(cx);
    press(cx, "j");
    press(cx, "j");
    press(cx, "enter");
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    render(cx);
    assert_eq!(label(cx, "issue-title").as_deref(), Some(TITLE));
    assert!(has(cx, "issue-back"));
    assert!(!has(cx, "issues-tabs"));
    cx.update(|_, cx| {
        assert_eq!(
            board.read(cx).issue_view.description.as_deref(),
            Some("Price \\$5. [Tracker](/issues/SUB-1) and text.")
        );
    });

    settle_board(&board, cx, |board| board.issue("issue:2").is_some());
    press(cx, "j");
    settle_board(&board, cx, |board| is_loaded(board, "SUB-2"));
    press(cx, "k");
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    press(cx, "escape");
    assert_eq!(selected(&board, cx).as_deref(), Some("issue:1"));
    assert!(has(cx, "issues-tabs"));
    assert!(!has(cx, "issues-detail"));
    cx.update(|window, cx| {
        assert!(board.read(cx).focus.is_focused(window));
        assert!(
            board
                .read(cx)
                .hub
                .as_ref()
                .unwrap()
                .snapshot()
                .issue()
                .is_none()
        );
    });
}

#[gpui_kit::test]
fn child_paging_recovers_and_offline_keeps_only_the_open_issue(cx: &mut TestAppContext) {
    let offline = offline();
    let (board, cx) = open_first(&offline, cx);
    render(cx);
    assert_eq!(
        label(cx, "issue-more-subissues").as_deref(),
        Some("Show 1 more")
    );

    offline
        .switches
        .fail_children
        .store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| board.more_subissues(cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .children_failed()
    });
    render(cx);
    assert_eq!(label(cx, "issue-more-subissues").as_deref(), Some("Retry"));
    offline
        .switches
        .fail_children
        .store(false, Ordering::Relaxed);
    board.update(cx, |board, cx| board.more_subissues(cx));
    settle_board(&board, cx, |board| children(board) == 2);

    offline
        .switches
        .offline_issue
        .store(true, Ordering::Relaxed);
    cx.update(|window, cx| board.update(cx, |board, cx| board.reload(window, cx)));
    settle_board(&board, cx, |board| {
        *board.issue_view.snapshot.as_ref().unwrap().status()
            == IssueStatus::Error(ReadError::Network)
    });
    render(cx);
    assert_eq!(label(cx, "issue-title").as_deref(), Some(TITLE));
    assert!(has(cx, "issue-offline-retry"));
    cx.update(|_, cx| {
        assert_eq!(children(board.read(cx)), 2);
    });
}

fn children(board: &Issues) -> usize {
    board
        .issue_view
        .snapshot
        .as_ref()
        .unwrap()
        .detail()
        .unwrap()
        .children
        .as_ref()
        .unwrap()
        .items
        .len()
}

mod back;
mod recovery;

#[gpui_kit::test]
fn a_relation_opens_the_related_issue_in_the_same_detail_view(cx: &mut TestAppContext) {
    use super::offline::{offline_with, row};
    use gpui_kit::test::TestWindowExt as _;
    let fixture = offline_with(|request| {
        if !request.path.ends_with("/issues/SUB-1") {
            return None;
        }
        let mut detail = row("1", "Fixture issue with relation");
        let mut related = row("2", "Fixture related issue");
        related["kind"] = serde_json::json!("blocks");
        detail["relations"] = serde_json::json!({"items":[related], "nextCursor":null});
        detail["todos"] = serde_json::json!([]);
        Some((200, detail.to_string()))
    });
    let (board, cx) = open_first(&fixture, cx);
    render(cx);
    cx.update(|window, cx| window.click("issue-relation:blocks:2", cx));
    settle_board(&board, cx, |board| is_loaded(board, "SUB-2"));
    render(cx);
    assert_eq!(
        label(cx, "issue-title").as_deref(),
        Some(super::offline::LATER_TITLE)
    );
}
