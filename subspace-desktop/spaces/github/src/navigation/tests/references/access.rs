//! One access rule for every read path: a refusal answers until a read
//! that started later succeeds, whichever path made each read, a result
//! from a read that started earlier is dropped, a refusal is read again on
//! the next refresh, and a rate limit is a failure, not a refusal.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::{ResolutionState, ResolvePullRequests};
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::contracts;

use super::super::loading::response;
use super::super::{KEY, seed};
use super::checks::{land_section_at, listed_only};
use super::{ASKER, Meanwhile, ask, heard, null_repository, response_with_checks, routed};
use crate::Github;

/// Another pull request in the seeded repository, which no list holds.
const OTHER: &str = "octo-org/api#7";

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
    land_section_at(&board, section, cx);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "a section asked for before the refusal changes nothing"
    );
    let later = tick(&board, cx);
    land_section_at(&board, later, cx);
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
    let meanwhile = Meanwhile::new(response_with_checks);
    let mock = &meanwhile.mock;
    let (board, _listener, cx) = routed(cx, Some(mock));
    meanwhile.arm(&board, cx);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Pending),
        "a reference read is queued"
    );
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1);
    // A detail read that started while the reference read's request was in
    // flight lands refused.
    detail_failed(&board, status(403), meanwhile.started(), cx);
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
    land_section_at(&board, section, cx);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1, "read again once");
    assert!(available(cx), "the newer read lifts it");
}

/// Asks for two pull requests in one request: one reference read of
/// both, each in turn.
fn ask_both(keys: [&str; 2], cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let keys = ResolvePullRequests::new(keys);
        let answers =
            contracts::request(ASKER, &keys, cx).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(answers.len(), 2);
    });
}

/// A reference read of another pull request, then of [`KEY`], in one
/// batch: the first is served, the second refused with a 403.
fn other_then_refused() -> Mock {
    let served = Arc::new(AtomicUsize::new(0));
    Mock::new(move |_| match served.fetch_add(1, Ordering::SeqCst) {
        0 => (200, response("PullRequest", 7)),
        _ => (403, r#"{"message":"Resource not accessible"}"#.to_owned()),
    })
}

#[gpui_kit::test]
fn a_batch_s_later_request_is_newer_than_a_read_started_while_it_waited(cx: &mut TestAppContext) {
    let mock = other_then_refused();
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    // One reference read of two pull requests is queued; KEY is listed, so
    // it answers from the list meanwhile.
    ask_both([OTHER, KEY], cx);
    // While the batch waits on its first key, this Space's own detail read
    // of KEY starts and succeeds; access is lost after it.
    let detail = tick(&board, cx);
    board.update(cx, |board, _| {
        let login = board.account.clone().unwrap_or_default();
        crate::references::detail_landed(board, &login, (KEY, KEY), detail);
    });
    // The batch reaches KEY only now, and GitHub refuses it.
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 2, "one request per key, in turn");
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "the refusal started after the detail read, so it answers"
    );
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

#[gpui_kit::test]
fn a_plain_403_from_github_is_an_access_refusal(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (403, r#"{"message":"Resource not accessible"}"#.to_owned()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "the held list's title no longer answers"
    );
}

#[gpui_kit::test]
fn a_pull_request_read_under_another_repository_name_is_never_cached_under_it(
    cx: &mut TestAppContext,
) {
    // GitHub answers a former repository name with the pull request under
    // its current name, which no other key reaches.
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    let (_board, _listener, cx) = routed(cx, Some(&mock));
    let alias = "octo-org/former-api#421";
    ask(alias, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1);
    assert!(
        matches!(ask(alias, cx), ResolutionState::Unavailable),
        "a summary GitHub gave under another key never answers for this one"
    );
}

#[gpui_kit::test]
fn a_detail_read_takes_its_tick_when_its_request_starts_not_when_queued(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (403, r#"{"message":"Resource not accessible"}"#.to_owned()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    // This Space's detail read of KEY is queued behind its debounce.
    board.update(cx, |board, cx| board.fetch_detail(KEY.into(), true, cx));
    // Meanwhile another read of KEY starts and succeeds; access is lost.
    let meanwhile = tick(&board, cx);
    board.update(cx, |board, _| {
        let login = board.account.clone().unwrap_or_default();
        crate::references::detail_landed(board, &login, (KEY, KEY), meanwhile);
    });
    // The detail request starts only now, and GitHub refuses it.
    cx.executor().advance_clock(crate::DETAIL_DEBOUNCE);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "the refusal started after the other read, so it answers"
    );
}

/// [`KEY`] refused, then another pull request served.
fn refused_then_other() -> Mock {
    let served = Arc::new(AtomicUsize::new(0));
    Mock::new(move |_| match served.fetch_add(1, Ordering::SeqCst) {
        0 => (403, r#"{"message":"Resource not accessible"}"#.to_owned()),
        _ => (200, response("PullRequest", 7)),
    })
}

#[gpui_kit::test]
fn each_request_of_a_batch_lands_and_is_announced_as_it_completes(cx: &mut TestAppContext) {
    let mock = refused_then_other();
    let (board, listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    ask_both([KEY, OTHER], cx);
    assert_eq!(
        heard(&listener, cx),
        2,
        "KEY's refusal is announced before OTHER is read, not with it"
    );
    assert_eq!(mock.requests().len(), 2);
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
}
