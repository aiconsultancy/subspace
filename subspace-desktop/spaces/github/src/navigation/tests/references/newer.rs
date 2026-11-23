//! Answers newer than what the account holds win, and only those: a
//! later list section over a read that began before it, and GitHub's
//! refusal over a pull request the account listed or opened before it,
//! until a later read succeeds.

use gpui_kit::TestAppContext;
use subspace_framework::client_github::model::CheckState;
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::{CheckStatus, ResolutionState};

use super::super::{KEY, seed};
use super::checks::{land_section, listed_only, read_once_then_denied, refresh, relist};
use super::{ask, response_with_checks, routed};

#[gpui_kit::test]
fn a_later_section_of_the_same_refresh_wins_over_a_read_that_started_before_it(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    // The refresh's first section lands and a tile asks: one read starts.
    refresh(&board, 1, cx);
    ask(KEY, cx);
    // A later section of the same refresh lands before the read does, with
    // the checks now passing.
    relist(&board, cx, |item| item.checks = Some(CheckState::Success));
    land_section(&board, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1);
    let ResolutionState::Available(details) = ask(KEY, cx) else {
        panic!("available from the list");
    };
    assert_eq!(
        details.checks,
        Some(CheckStatus::Success),
        "the later section's checks, not the older read's failure"
    );
    assert!(details.check_counts.is_none(), "counts for other checks");
}

#[gpui_kit::test]
fn a_listed_pull_request_the_account_lost_access_to_stops_showing_its_summary(
    cx: &mut TestAppContext,
) {
    let mock = read_once_then_denied();
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    ask(KEY, cx);
    cx.run_until_parked();
    assert!(matches!(ask(KEY, cx), ResolutionState::Available(_)));
    // The refresh still lists it from before; the read again is refused.
    refresh(&board, 1, cx);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 2);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "the refusal, not the list's title and status"
    );
    cx.run_until_parked();
    assert_eq!(
        mock.requests().len(),
        2,
        "no read again until the next refresh"
    );
}

#[gpui_kit::test]
fn an_opened_pull_request_refused_since_answers_with_the_refusal_until_read_again(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, seed);
    assert!(matches!(ask(KEY, cx), ResolutionState::Available(_)));
    board.update(cx, |board, cx| {
        let login = board.account.clone().unwrap_or_default();
        let denied = subspace_framework::core::http::Error::Status {
            status: 404,
            message: "Not Found".into(),
        };
        let tick = crate::references::begin(board);
        crate::references::detail_refused(board, (&login, KEY), &denied, tick, cx);
    });
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "the refusal, not the detail opened before"
    );
    board.update(cx, |board, _| {
        let login = board.account.clone().unwrap_or_default();
        let tick = crate::references::begin(board);
        crate::references::detail_landed(board, &login, (KEY, KEY), tick);
    });
    assert!(matches!(ask(KEY, cx), ResolutionState::Available(_)));
}
