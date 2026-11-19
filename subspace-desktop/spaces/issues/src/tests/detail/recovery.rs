use std::sync::atomic::Ordering;

use gpui_kit::{TestAppContext, VisualTestContext};
use subspace_api_client::hub::{IssueStatus, ReadError};
use subspace_framework::dashboard::Dashboard as _;

use super::{has, is_loaded, label, render, settle_board};
use crate::tests::offline::{Offline, board_with_hub, offline};

const ISSUE: &str = "SUB-1";
const ROW: &str = "issue:1";

#[gpui_kit::test]
fn a_missing_issue_is_neutral_and_can_return_to_the_list(cx: &mut TestAppContext) {
    let fixture = offline();
    let cx = show_error(&fixture, 404, IssueStatus::Unavailable, cx);
    assert_eq!(
        label(cx, "issues-detail-unavailable").as_deref(),
        Some("SUB-1 isn't available")
    );
    assert!(has(cx, "issue-back"));
    assert!(!has(cx, "issues-retry"));
}

#[gpui_kit::test]
fn a_network_error_offers_retry_and_copy_details(cx: &mut TestAppContext) {
    let fixture = offline();
    let cx = show_error(
        &fixture,
        500,
        IssueStatus::Error(ReadError::Response { status: Some(500) }),
        cx,
    );
    assert_eq!(
        label(cx, "issues-detail-error").as_deref(),
        Some("Couldn't load SUB-1")
    );
    assert!(has(cx, "issues-retry"));
    assert!(has(cx, "issues-copy-details"));
}

#[gpui_kit::test]
fn sign_in_needed_offers_retry_without_sign_in(cx: &mut TestAppContext) {
    let fixture = offline();
    let cx = show_error(
        &fixture,
        401,
        IssueStatus::Error(ReadError::SignInNeeded),
        cx,
    );
    assert_eq!(
        label(cx, "issues-detail-error").as_deref(),
        Some("Couldn't load SUB-1")
    );
    assert!(has(cx, "issues-retry"));
    assert!(!has(cx, "issues-sign-in"));
    assert!(!has(cx, "issue-title"));
}

#[gpui_kit::test]
fn access_retry_reloads_the_issue_when_access_returns(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue(ROW).is_some());
    fixture.switches.issue_error.store(403, Ordering::Relaxed);
    board.update(cx, |board, cx| board.open_row(ROW, cx));
    settle_board(&board, cx, |board| {
        board.issue_view.snapshot.as_ref().is_some_and(|issue| {
            matches!(issue.status(), IssueStatus::Error(ReadError::SignInNeeded))
        })
    });
    render(cx);
    assert_eq!(
        label(cx, "issues-detail-error").as_deref(),
        Some("Couldn't load SUB-1")
    );
    assert!(!has(cx, "issue-title"));
    assert!(has(cx, "issues-retry"));
    fixture.switches.issue_error.store(0, Ordering::Relaxed);
    board.update(cx, |board, cx| board.reload_detail(cx));
    settle_board(&board, cx, |board| is_loaded(board, ISSUE));
    render(cx);
    assert!(has(cx, "issue-title"));
}

#[gpui_kit::test]
fn a_pending_issue_has_loading_and_back(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue(ROW).is_some());
    fixture.switches.hold_issue.store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| board.open_row(ROW, cx));
    render(cx);
    fixture.switches.hold_issue.store(false, Ordering::Relaxed);
    assert_eq!(
        label(cx, "issues-detail-loading").as_deref(),
        Some("Loading SUB-1…")
    );
    assert!(has(cx, "issue-back"));
}

#[gpui_kit::test]
fn hiding_clears_an_issue_the_board_has_not_observed(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue(ROW).is_some());
    let hub = cx.update(|_, cx| board.read(cx).hub.clone().unwrap());
    hub.open_issue(ISSUE, "subspace-api").unwrap();
    cx.update(|window, cx| {
        assert!(board.read(cx).issue_view.snapshot.is_none());
        board.update(cx, |board, cx| board.deactivate(window, cx));
    });
    assert!(hub.snapshot().issue().is_none());
}

fn show_error<'a>(
    fixture: &Offline,
    code: u16,
    expected: IssueStatus,
    cx: &'a mut TestAppContext,
) -> &'a mut VisualTestContext {
    let (board, cx) = board_with_hub(fixture, cx);
    settle_board(&board, cx, |board| board.issue(ROW).is_some());
    fixture.switches.issue_error.store(code, Ordering::Relaxed);
    board.update(cx, |board, cx| board.open_row(ROW, cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| *issue.status() == expected)
    });
    render(cx);
    cx
}
