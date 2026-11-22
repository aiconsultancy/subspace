//! A pull request the account holds only from a list answers at once from
//! the list, then with its check counts once one read of it lands, and is
//! read again at most once per list refresh.

use gpui_kit::{Context, Entity, TestAppContext, VisualTestContext};
use subspace_framework::client_github::model::{CheckState, GhItem, PrState};
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::github::{CheckStatus, PullRequestStatus, ResolutionState};

use subspace_framework::dashboard::github_sign_in::SignInEvent;

use super::super::{KEY, seed};
use super::{ask, emit, heard, null_repository, response_with_checks, routed};
use crate::Github;

/// The seeded pull request held only from the list.
pub(super) fn listed_only(board: &mut Github, cx: &mut Context<Github>) {
    seed(board, cx);
    if let Some(account) = board.data.get_mut("octo-dev") {
        account.details.clear();
    }
}

/// Changes the listed item, as a refreshed list would.
pub(super) fn relist(
    board: &Entity<Github>,
    cx: &mut VisualTestContext,
    change: impl Fn(&mut GhItem),
) {
    board.update(cx, |board, _| {
        let account = board.data.get_mut("octo-dev").unwrap();
        for section in account.sections.values_mut() {
            section.items.iter_mut().for_each(&change);
        }
    });
}

/// A refresh of the lists starts, and `sections` of it land, each listing
/// the seeded pull request.
pub(super) fn refresh(board: &Entity<Github>, sections: usize, cx: &mut VisualTestContext) {
    board.update(cx, |board, _| crate::references::refreshing(board));
    for _ in 0..sections {
        land_section(board, cx);
    }
}

/// A pull request read once, then denied, as when the account lost access
/// to its repository.
pub(super) fn read_once_then_denied() -> Mock {
    let served = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    Mock::new(
        move |_| match served.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
            0 => (200, response_with_checks()),
            _ => (200, null_repository()),
        },
    )
}

/// One section of a refresh lands, listing the seeded pull request.
pub(super) fn land_section(board: &Entity<Github>, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        let login = board.account.clone().unwrap_or_default();
        let since = crate::references::since(board);
        let section = board.section;
        crate::references::section_landed(board, &login, section, since, cx);
    });
}

/// The list now says what the last read said: its checks state and last
/// update, as a refresh after it would.
fn confirm(board: &Entity<Github>, cx: &mut VisualTestContext) {
    let ResolutionState::Available(read) = ask(KEY, cx) else {
        panic!("read");
    };
    relist(board, cx, |item| {
        item.checks = Some(CheckState::Failure);
        item.updated_at = read.updated_at.clone().unwrap_or_default();
    });
}

fn counts(cx: &mut VisualTestContext) -> Option<(u32, u32)> {
    match ask(KEY, cx) {
        ResolutionState::Available(details) => details.check_counts.map(|c| (c.passed, c.total)),
        _ => None,
    }
}

#[gpui_kit::test]
fn a_listed_pull_request_answers_at_once_then_with_its_check_counts_once_read(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    let ResolutionState::Available(details) = ask(KEY, cx) else {
        panic!("a listed pull request is available at once");
    };
    assert_eq!(details.checks, Some(CheckStatus::Pending), "from the list");
    assert!(details.check_counts.is_none(), "the list has no runs");
    assert_eq!(heard(&listener, cx), 1, "the read of its runs is announced");
    assert_eq!(counts(cx), Some((4, 5)), "the tile now has its bar");
    assert_eq!(counts(cx), Some((4, 5)));
    assert_eq!(
        mock.requests().len(),
        1,
        "one read, however often it is asked"
    );
    // A list refresh lands, confirming the failure the read found: the
    // next ask reads it again, once, and keeps answering with the counts it
    // has meanwhile.
    confirm(&board, cx);
    refresh(&board, 1, cx);
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

#[gpui_kit::test]
fn a_listed_pull_request_is_read_once_per_refresh_however_many_sections_land_or_opens(
    cx: &mut TestAppContext,
) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(counts(cx), Some((4, 5)));
    assert_eq!(mock.requests().len(), 1);
    // Detail reads land as someone opens pull requests: nothing is stale.
    board.update(cx, |board, cx| {
        let login = board.account.clone().unwrap_or_default();
        for _ in 0..20 {
            let since = crate::references::since(board);
            crate::references::landed(board, &login, since, cx);
        }
    });
    for _ in 0..3 {
        ask(KEY, cx);
        cx.run_until_parked();
    }
    assert_eq!(
        mock.requests().len(),
        1,
        "opening pull requests reads nothing again"
    );
    // One refresh whose four sections land a moment apart, Channels asking
    // after each: one read.
    confirm(&board, cx);
    board.update(cx, |board, _| crate::references::refreshing(board));
    for _ in 0..4 {
        land_section(&board, cx);
        ask(KEY, cx);
        cx.run_until_parked();
    }
    assert_eq!(mock.requests().len(), 2, "one read for the whole refresh");
    refresh(&board, 3, cx);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(
        mock.requests().len(),
        3,
        "the next refresh reads it once more"
    );
}

#[gpui_kit::test]
fn a_stale_read_never_overrides_the_newer_list(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(
        counts(cx),
        Some((4, 5)),
        "a read since the list answers whole"
    );
    // The pull request merged, its checks now passing, as the refreshed list
    // says; the read from before answers neither its status nor its counts.
    relist(&board, cx, |item| {
        item.state = PrState::Merged;
        item.checks = Some(CheckState::Success);
    });
    refresh(&board, 1, cx);
    let ResolutionState::Available(details) = ask(KEY, cx) else {
        panic!("still available from the list");
    };
    assert_eq!(
        details.status,
        PullRequestStatus::Merged,
        "the list's status"
    );
    assert_eq!(
        details.checks,
        Some(CheckStatus::Success),
        "the list's checks"
    );
    assert!(
        details.check_counts.is_none(),
        "old counts for other checks are not shown"
    );
}

#[gpui_kit::test]
fn a_force_push_never_shows_the_old_head_s_counts(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    ask(KEY, cx);
    cx.run_until_parked();
    confirm(&board, cx);
    // A new head with the same combined state: the list says it changed.
    relist(&board, cx, |item| {
        item.updated_at = "2027-01-01T00:00:00Z".into()
    });
    refresh(&board, 1, cx);
    let ResolutionState::Available(details) = ask(KEY, cx) else {
        panic!("still available from the list");
    };
    assert!(details.check_counts.is_none(), "the old head's counts");
    assert_eq!(details.updated_at.as_deref(), Some("2027-01-01T00:00:00Z"));
}

#[gpui_kit::test]
fn a_signed_out_account_queues_no_check_count_read(cx: &mut TestAppContext) {
    let mock = Mock::new(|_| (200, response_with_checks()));
    let (board, _listener, cx) = routed(cx, Some(&mock));
    board.update(cx, listed_only);
    emit(SignInEvent::SignedOut("octo-dev".into()), cx);
    assert!(matches!(ask(KEY, cx), ResolutionState::SignedOut));
    cx.run_until_parked();
    assert!(
        mock.requests().is_empty(),
        "no read for a signed-out account"
    );
}

#[gpui_kit::test]
fn a_pull_request_that_left_the_lists_is_checked_again_once_per_refresh(cx: &mut TestAppContext) {
    // Read once, then GitHub denies it, as when the account lost access to
    // its repository.
    let mock = read_once_then_denied();
    let (board, _listener, cx) = routed(cx, Some(&mock));
    ask(KEY, cx);
    cx.run_until_parked();
    assert!(matches!(ask(KEY, cx), ResolutionState::Available(_)));
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 1, "no refresh, no read");
    refresh(&board, 2, cx);
    ask(KEY, cx);
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 2, "checked again once");
    assert!(
        matches!(ask(KEY, cx), ResolutionState::Unavailable),
        "no longer answered from memory"
    );
    cx.run_until_parked();
    assert_eq!(mock.requests().len(), 2);
}
