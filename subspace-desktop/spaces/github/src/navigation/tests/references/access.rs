//! One access rule for every read path: a refusal answers until a read
//! that started later succeeds, whichever path made each read, a result
//! from a read that started earlier is dropped, a refusal is read again on
//! the next refresh, and a rate limit is a failure, not a refusal.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::core::http::Error;

use super::super::{KEY, seed};
use super::checks::listed_only;
use super::{ask, null_repository, response_with_checks, routed};
use crate::Github;

fn tick(board: &Entity<Github>, cx: &mut VisualTestContext) -> u64 {
    board.update(cx, |board, _| crate::references::begin(board))
}

fn status(code: u16) -> Error {
    Error::Status {
        status: code,
        message: "Not Found".into(),
    }
}

/// This Space's own detail read of [`KEY`], started at `started`, failed
/// with `error`.
fn detail_failed(board: &Entity<Github>, error: Error, started: u64, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        let login = board.account.clone().unwrap_or_default();
        crate::references::detail_refused(board, (&login, KEY), &error, started, cx);
    });
}

/// The seeded section, started at `started`, lands.
fn section_at(board: &Entity<Github>, started: u64, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        let login = board.account.clone().unwrap_or_default();
        let since = crate::references::since(board);
        let section = board.section;
        crate::references::section_landed(board, &login, (section, started), since, cx);
    });
}

fn available(cx: &mut VisualTestContext) -> bool {
    matches!(ask(KEY, cx), ResolutionState::Available(_))
}

#[gpui_kit::test]
fn a_section_requested_before_a_refusal_never_brings_the_summary_back(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    let section = tick(&board, cx);
    let refused = tick(&board, cx);
    detail_failed(&board, status(404), refused, cx);
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    section_at(&board, section, cx);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "a section asked for before the refusal changes nothing"
    );
    let later = tick(&board, cx);
    section_at(&board, later, cx);
    assert!(available(cx), "a section asked for after it lifts it");
    cx.run_until_parked();
    assert_eq!(
        mock.requests().len(),
        1,
        "only the listed pull request's one read for its check runs"
    );
}

#[gpui_kit::test]
fn an_older_reference_read_never_overrides_a_newer_detail_refusal(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Pending),
        "a reference read starts"
    );
    let refused = tick(&board, cx);
    detail_failed(&board, status(403), refused, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "the older summary is dropped"
    );
    cx.run_until_parked();
    assert_eq!(
        mock.requests().len(),
        1,
        "and not read again before a refresh"
    );
}

#[gpui_kit::test]
fn an_older_detail_success_never_lifts_a_newer_reference_refusal(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    let detail = tick(&board, cx);
    ask(KEY, cx);
    cx.run_until_parked();
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    board.update(cx, |board, _| {
        let login = board.account.clone().unwrap_or_default();
        crate::references::detail_landed(board, &login, (KEY, KEY), detail);
    });
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    assert_eq!(mock.requests().len(), 1);
}

#[gpui_kit::test]
fn an_opened_pull_request_s_refusal_is_read_again_on_the_next_refresh(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, |board, cx| {
        seed(board, cx);
        if let Some(account) = board.data.get_mut("octo-dev") {
            account.sections.clear();
        }
    });
    let refused = tick(&board, cx);
    detail_failed(&board, status(404), refused, cx);
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 0, "not before a refresh");
    board.update(cx, |board, _| crate::references::refreshing(board));
    let section = tick(&board, cx);
    section_at(&board, section, cx);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1, "read again once");
    assert!(available(cx), "the newer read lifts it");
}

#[gpui_kit::test]
fn a_rate_limit_is_a_failure_not_a_refusal(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, seed);
    let limited = tick(&board, cx);
    detail_failed(&board, status(429), limited, cx);
    assert!(available(cx), "the opened pull request still answers");
}
