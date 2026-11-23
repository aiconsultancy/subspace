//! A forget (a sign-in or sign-out, an account switch, a reload) clears
//! every answer but keeps the access decisions; a refused pull request is
//! still read again on the next refresh.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::TestAppContext;
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::dashboard::github_sign_in::SignInEvent;

use super::super::loading::response;
use super::super::{KEY, seed};
use super::checks::{land_section, land_section_at};
use super::{ask, emit, heard, null_repository, routed};

#[gpui_kit::test]
fn another_account_signing_out_keeps_this_account_s_refusal_until_the_next_refresh(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response("PullRequest", 421)));
    let (board, listener, cx) = routed(cx, Some(&mock));
    board.update(cx, |board, cx| {
        seed(board, cx);
        if let Some(account) = board.data.get_mut("octo-dev") {
            account.sections.clear();
        }
    });
    // Opened, then refused: the refusal answers.
    board.update(cx, |board, cx| {
        let login = board.account.clone().unwrap_or_default();
        let started = crate::references::begin(board);
        let denied = subspace_framework::core::http::Error::Status {
            status: 404,
            message: "Not Found".into(),
        };
        crate::references::detail_refused(board, (&login, KEY), &denied, started, cx);
    });
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    emit(SignInEvent::SignedOut("other-account".into()), cx);
    heard(&listener, cx);
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "the retained title never answers again without a newer read"
    );
    heard(&listener, cx);
    assert!(mock.requests().is_empty(), "no read before a refresh");
    // The forget cleared every answer; the next refresh still reads the
    // refused pull request again, and the newer success lifts the refusal.
    board.update(cx, |board, _| crate::references::refreshing(board));
    land_section(&board, cx);
    ask(KEY, cx);
    heard(&listener, cx);
    assert_eq!(mock.requests().len(), 1, "read again once on the refresh");
    assert!(matches!(ask(KEY, cx), ResolutionState::Available(_)));
}

/// Refused once, then served, as when access comes back.
fn refused_then_served() -> Mock {
    let served = Arc::new(AtomicUsize::new(0));
    Mock::new(move |_| match served.fetch_add(1, Ordering::SeqCst) {
        0 => (200, null_repository()),
        _ => (200, response("PullRequest", 421)),
    })
}

#[gpui_kit::test]
fn a_refusal_is_read_again_after_a_forget_and_a_refresh(cx: &mut TestAppContext) {
    let mock = refused_then_served();
    let (board, _listener, cx) = routed(cx, Some(&mock));
    // A reference read of a pull request the account does not hold is
    // refused.
    ask(KEY, cx);
    cx.run_until_parked();
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    // Another account signing in forgets every answer; the refusal stays.
    emit(SignInEvent::SignedIn("other-account".into()), cx);
    cx.run_until_parked();
    assert!(matches!(ask(KEY, cx), ResolutionState::Unavailable));
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1, "no read before a refresh");
    // The next refresh reads it again, and access is back.
    board.update(cx, |board, _| crate::references::refreshing(board));
    let section = board.update(cx, |board, _| crate::references::begin(board));
    land_section_at(&board, section, cx);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 2, "read again once");
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Available(_)),
        "the newer read lifts the refusal"
    );
}
