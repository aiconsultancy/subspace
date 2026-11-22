//! A pull request the account holds only from a list answers at once from
//! the list, then with its check counts once one read of it lands, and is
//! read again at most once per list refresh.

use gpui_kit::{TestAppContext, VisualTestContext};
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::{CheckStatus, ResolutionState};

use super::super::{KEY, seed};
use super::{ask, heard, response_with_checks, routed};

#[gpui_kit::test]
fn a_listed_pull_request_answers_at_once_then_with_its_check_counts_once_read(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, listener, cx) = routed(cx, Some(&mock));
    board.update(cx, |board, cx| {
        seed(board, cx);
        // Held only from the list: not opened.
        if let Some(account) = board.data.get_mut("octo-dev") {
            account.details.clear();
        }
    });
    let ResolutionState::Available(details) = ask(KEY, cx) else {
        panic!("a listed pull request is available at once");
    };
    assert_eq!(details.checks, Some(CheckStatus::Pending), "from the list");
    assert!(details.check_counts.is_none(), "the list has no runs");
    assert_eq!(heard(&listener, cx), 1, "the read of its runs is announced");
    let counts = |cx: &mut VisualTestContext| match ask(KEY, cx) {
        ResolutionState::Available(details) => details.check_counts.map(|c| (c.passed, c.total)),
        _ => None,
    };
    assert_eq!(counts(cx), Some((4, 5)), "the tile now has its bar");
    assert_eq!(counts(cx), Some((4, 5)));
    assert_eq!(
        mock.requests().len(),
        1,
        "one read, however often it is asked"
    );
    // A list refresh lands: the next ask reads it again, once, and keeps
    // answering with the counts it has meanwhile.
    board.update(cx, |board, cx| {
        let login = board.account.clone().unwrap_or_default();
        let since = crate::references::since(board);
        crate::references::landed(board, &login, since, cx);
    });
    assert_eq!(
        counts(cx),
        Some((4, 5)),
        "no bar lost while it is read again"
    );
    cx.run_until_parked();
    assert_eq!(counts(cx), Some((4, 5)));
    assert_eq!(
        mock.requests().len(),
        2,
        "read again once after the refresh"
    );
}
