//! Checks of a move whose reply was lost: once the root reads back in the
//! destination, the move committed, whatever its sub-issues show.
use super::UnreconciledReason;
use super::reconcile_tests::{CHILD, ROOT, issue, patches, plan, unreconciled};
use crate::Client;
use crate::issue_moves::MovePreview;
use std::time::{Duration, Instant};
use subspace_framework::client_test_support::Mock;

/// A NETT where the move of ROOT, with `children`, committed but its reply
/// was lost: ROOT reads back in WEB, and each sub-issue as `child` answers.
fn committed_unanswered(
    children: &[(&str, &str)],
    child: impl Fn() -> (u16, String) + Send + Sync + 'static,
) -> (Mock, Client, MovePreview) {
    let mock = Mock::new(move |request| {
        let path = request.path.as_str();
        if path.ends_with("/desktop/capabilities") {
            (200, r#"{"features":["issueMoves"]}"#.into())
        } else if path.ends_with(&format!("/issues/{ROOT}")) {
            (
                200,
                issue(ROOT, "WEB-7", ("team-web", "WEB"), 3).to_string(),
            )
        } else if path.contains("/issues/") {
            child()
        } else {
            (404, "{}".into())
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

#[test]
fn a_check_that_reads_the_root_moved_reports_the_move_committed() {
    // The reply was lost, and a teammate has since moved the sub-issue on.
    let (mock, client, preview) = committed_unanswered(&[(CHILD, "OPS-3")], || {
        (
            200,
            issue(CHILD, "DOC-9", ("team-docs", "DOC"), 4).to_string(),
        )
    });
    let result = client
        .verify_issue_move(&preview)
        .unwrap_or_else(|error| panic!("the move committed, not {error:?}"));
    assert_eq!(result.issue.fields.identifier, "WEB-7");
    assert_eq!(
        result.unreconciled,
        unreconciled(UnreconciledReason::Elsewhere)
    );
    assert_eq!(patches(&mock), 0, "a check sends no move");
}

#[test]
fn a_check_of_a_large_move_finishes_once_it_reads_the_root() {
    // Far more sub-issues than the budget can read, each slow to answer.
    let ids: Vec<String> = (0..40)
        .map(|n| format!("00000000-0000-4000-8000-0000000002{n:02}"))
        .collect();
    let children: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), "OPS-9")).collect();
    let (_mock, client, preview) = committed_unanswered(&children, || {
        std::thread::sleep(Duration::from_millis(150));
        (404, "{}".into())
    });
    let started = Instant::now();
    let result = client
        .nett_verify_move_within(&preview, None, Duration::from_millis(600))
        .unwrap_or_else(|error| panic!("the move committed, not {error:?}"));
    assert!(started.elapsed() < Duration::from_millis(1500));
    assert_eq!(result.unreconciled.len(), 40, "every sub-issue is listed");
    assert!(
        result
            .unreconciled
            .iter()
            .any(|item| item.reason == UnreconciledReason::NotChecked),
        "those the time ran out for are said to be"
    );
}
