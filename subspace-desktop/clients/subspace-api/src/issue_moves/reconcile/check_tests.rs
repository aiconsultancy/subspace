//! Checks of a move whose reply was lost: a root read back in the
//! destination is this plan's move only when its sub-issues tie it to the
//! plan.
use super::UnreconciledReason;
use super::reconcile_tests::{CHILD, ROOT, SECOND, issue, patches, plan, unreconciled};
use crate::Client;
use crate::issue_moves::MovePreview;
use crate::issue_writes::IssueSaveError;
use std::time::{Duration, Instant};
use subspace_framework::client_test_support::Mock;

/// A NETT where ROOT reads back moved into WEB, its reply lost, and a read
/// of any other issue is answered by `child`, given that issue's ID.
fn committed_unanswered(
    children: &[(&str, &str)],
    child: impl Fn(&str) -> (u16, String) + Send + Sync + 'static,
) -> (Mock, Client, MovePreview) {
    let mock = Mock::new(move |request| {
        let path = request.path.as_str();
        match path.rsplit_once("/issues/") {
            _ if path.ends_with("/desktop/capabilities") => {
                (200, r#"{"features":["issueMoves"]}"#.into())
            }
            Some((_, ROOT)) => (
                200,
                issue(ROOT, "WEB-7", ("team-web", "WEB"), 3).to_string(),
            ),
            Some((_, id)) => child(id),
            None => (404, "{}".into()),
        }
    });
    let client = Client::nett_loopback(
        &format!("{}/api", mock.base_url()),
        Some("00000000-0000-4000-8000-0000000000e0"),
        Some(subspace_framework::core::Secret::new("nett-move-check")),
    )
    .unwrap()
    .without_live();
    (mock, client, plan(children))
}

/// The sub-issue `id` read back in `team`, newer than previewed.
fn read_in(id: &str, identifier: &str, team: (&str, &str)) -> (u16, String) {
    (200, issue(id, identifier, team, 4).to_string())
}

#[test]
fn a_check_that_reads_the_root_moved_with_its_sub_issues_reports_the_move_committed() {
    // The reply was lost, and a teammate has since moved one sub-issue on.
    let (mock, client, preview) =
        committed_unanswered(&[(CHILD, "OPS-3"), (SECOND, "OPS-4")], |id| match id {
            CHILD => read_in(CHILD, "DOC-9", ("team-docs", "DOC")),
            _ => read_in(id, "WEB-8", ("team-web", "WEB")),
        });
    let result = client
        .verify_issue_move(&preview)
        .unwrap_or_else(|error| panic!("the move committed, not {error:?}"));
    assert_eq!(result.issue.fields.identifier, "WEB-7");
    assert_eq!(result.moved.len(), 2, "the root and the sub-issue with it");
    assert_eq!(
        result.unreconciled,
        unreconciled(UnreconciledReason::Elsewhere)
    );
    assert_eq!(patches(&mock), 0, "a check sends no move");
}

#[test]
fn a_check_that_finds_a_sub_issue_still_where_it_was_is_a_conflict() {
    // The move never reached NETT: a teammate detached a sub-issue, then
    // moved the root to the same team on their own.
    let (mock, client, preview) =
        committed_unanswered(&[(CHILD, "OPS-3"), (SECOND, "OPS-4")], |id| match id {
            CHILD => read_in(CHILD, "OPS-3", ("team-ops", "OPS")),
            _ => read_in(id, "WEB-8", ("team-web", "WEB")),
        });
    assert_eq!(
        client.verify_issue_move(&preview).err(),
        Some(IssueSaveError::Conflict)
    );
    assert_eq!(patches(&mock), 0, "a check sends no move");
}

#[test]
fn a_check_with_no_sub_issue_moved_with_the_root_is_a_conflict() {
    // Every sub-issue was read, and none is in the destination.
    let (_mock, client, preview) = committed_unanswered(&[(CHILD, "OPS-3")], |id| {
        read_in(id, "DOC-9", ("team-docs", "DOC"))
    });
    assert_eq!(
        client.verify_issue_move(&preview).err(),
        Some(IssueSaveError::Conflict)
    );
}

#[test]
fn a_check_that_cannot_read_a_sub_issue_leaves_the_move_unconfirmed() {
    let (_mock, client, preview) =
        committed_unanswered(&[(CHILD, "OPS-3")], |_| (503, "{}".into()));
    assert_eq!(
        client.verify_issue_move(&preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
}

#[test]
fn a_check_of_a_large_move_finishes_once_a_sub_issue_ties_it_to_the_plan() {
    // Far more sub-issues than the budget can read, each slow to answer.
    let ids: Vec<String> = (0..40)
        .map(|n| format!("00000000-0000-4000-8000-0000000002{n:02}"))
        .collect();
    let children: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), "OPS-9")).collect();
    let (_mock, client, preview) = committed_unanswered(&children, |id| {
        std::thread::sleep(Duration::from_millis(150));
        read_in(id, "WEB-9", ("team-web", "WEB"))
    });
    let started = Instant::now();
    let result = client
        .nett_verify_move_within(&preview, None, Duration::from_millis(600))
        .unwrap_or_else(|error| panic!("the move committed, not {error:?}"));
    assert!(started.elapsed() < Duration::from_millis(1500));
    assert!(result.moved.len() > 1, "a sub-issue read moved ties it");
    assert_eq!(result.moved.len() + result.unreconciled.len(), 41);
    assert!(
        result
            .unreconciled
            .iter()
            .all(|item| item.reason == UnreconciledReason::NotChecked),
        "those the time ran out for are said to be"
    );
}
