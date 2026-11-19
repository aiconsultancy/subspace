//! A signed-out account stays signed out for every other Space
//! review P1): only its own sign-in or a successful read with it clears
//! that. Another account's sign-in or sign-out, or choosing the account
//! again, never does.

use std::time::Instant;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::github_sign_in::SignInEvent;

use super::refresh::{V2, github_v2};
use super::{ask, emit, label, null_repository, routed, title};
use crate::Github;
use crate::navigation::tests::{KEY, seed};

/// The current account's pull request is loaded and answers, then that
/// account signs out.
fn signed_out_after_loading<'a>(
    cx: &'a mut TestAppContext,
    mock: &Mock,
) -> (Entity<Github>, &'a mut VisualTestContext) {
    let (board, _, cx) = routed(cx, Some(mock));
    board.update(cx, |board, cx| {
        seed(board, cx);
        let account = board.data.get_mut("octo-dev").expect("the seeded account");
        account.loaded_at = Some(Instant::now());
    });
    assert!(title(&ask(KEY, cx)).is_some(), "loaded before the sign-out");
    emit(SignInEvent::SignedOut("octo-dev".into()), cx);
    cx.run_until_parked();
    assert_signed_out(cx, mock);
    (board, cx)
}

fn assert_signed_out(cx: &mut VisualTestContext, mock: &Mock) {
    let state = ask(KEY, cx);
    cx.run_until_parked();
    assert!(
        matches!(state, ResolutionState::SignedOut),
        "the signed-out account answers signed out, not {}",
        label(&state)
    );
    assert!(title(&state).is_none());
    assert!(
        mock.requests().is_empty(),
        "no read for a signed-out account"
    );
}

/// Another account's sign-in event leaves the signed-out account alone.
fn after_another_accounts(event: SignInEvent, cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (_, cx) = signed_out_after_loading(cx, &mock);
    emit(event, cx);
    cx.run_until_parked();
    assert_signed_out(cx, &mock);
}

#[gpui_kit::test]
fn another_account_signing_in_keeps_the_signed_out_account_signed_out(cx: &mut TestAppContext) {
    after_another_accounts(SignInEvent::SignedIn("other-account".into()), cx);
}

#[gpui_kit::test]
fn another_account_signing_out_keeps_the_signed_out_account_signed_out(cx: &mut TestAppContext) {
    after_another_accounts(SignInEvent::SignedOut("other-account".into()), cx);
}

#[gpui_kit::test]
fn choosing_the_signed_out_account_again_keeps_it_signed_out(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (board, cx) = signed_out_after_loading(cx, &mock);
    board.update(cx, |board, cx| board.set_account("octo-dev".into(), cx));
    cx.run_until_parked();
    assert_signed_out(cx, &mock);
    board.update(cx, |board, cx| board.set_account("OCTO-DEV".into(), cx));
    cx.run_until_parked();
    assert_signed_out(cx, &mock);
}

#[gpui_kit::test]
fn switching_away_and_back_keeps_the_signed_out_account_signed_out(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, null_repository()));
    let (board, cx) = signed_out_after_loading(cx, &mock);
    board.update(cx, |board, cx| {
        board.set_account("other-account".into(), cx);
        board.set_account("octo-dev".into(), cx);
    });
    // The switch reloads the lists; only reference reads matter here.
    let before = mock.requests().len();
    let state = ask(KEY, cx);
    cx.run_until_parked();
    assert!(
        matches!(state, ResolutionState::SignedOut),
        "{}",
        label(&state)
    );
    let reads = mock.requests()[before..]
        .iter()
        .filter(|request| request.body.contains("issueOrPullRequest"))
        .count();
    assert_eq!(reads, 0, "no read for a signed-out account");
}

#[gpui_kit::test]
fn a_refresh_that_started_before_the_sign_out_never_lifts_it(cx: &mut TestAppContext) {
    let mock = Mock::new(github_v2);
    let (board, _, cx) = routed(cx, Some(&mock));
    board.update(cx, seed);
    board.update_in(cx, |board, window, cx| board.reload(window, cx));
    emit(SignInEvent::SignedOut("octo-dev".into()), cx);
    cx.run_until_parked();
    let state = ask(KEY, cx);
    assert!(
        matches!(state, ResolutionState::SignedOut),
        "{}",
        label(&state)
    );
}

#[gpui_kit::test]
fn a_refresh_that_succeeds_after_the_sign_out_lifts_it(cx: &mut TestAppContext) {
    let mock = Mock::new(github_v2);
    let (board, cx) = signed_out_after_loading(cx, &mock);
    board.update_in(cx, |board, window, cx| board.reload(window, cx));
    cx.run_until_parked();
    assert_eq!(
        title(&ask(KEY, cx)).as_deref(),
        Some(V2),
        "the account's credentials worked again"
    );
}
