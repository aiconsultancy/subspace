use super::{Unreconciled, UnreconciledReason};
use crate::issue_moves::{MovePreview, MoveResult};
use crate::issue_writes::IssueSaveError;
use crate::{Client, Error, native::test_lease};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use subspace_framework::client_test_support::Mock;

pub(super) const ROOT: &str = "00000000-0000-4000-8000-0000000000e1";
pub(super) const CHILD: &str = "00000000-0000-4000-8000-0000000000e2";
const SECOND: &str = "00000000-0000-4000-8000-0000000000e3";

pub(super) fn issue(id: &str, identifier: &str, (team, key): (&str, &str), version: i64) -> Value {
    let mut issue = crate::tests::detail(id);
    issue["identifier"] = json!(identifier);
    issue["projectId"] = json!(team);
    issue["projectKey"] = json!(key);
    issue["version"] = json!(version);
    issue
}

fn item(id: &str, identifier: &str) -> Value {
    json!({"id": id, "version": 2, "oldIdentifier": identifier, "newIdentifier": null,
        "stateId": "state-1", "stateName": "Started", "cycleCleared": false,
        "milestoneCleared": false, "estimateCleared": false, "parentDetached": false,
        "archived": false})
}

/// A NETT that commits a move of ROOT to WEB, answering a read of a
/// sub-issue with `child`, and a client of it with the previewed plan:
/// ROOT, then each of `children`.
fn committing(
    children: &[(&str, &str)],
    child: impl Fn() -> (u16, String) + Send + Sync + 'static,
) -> (Mock, Client, MovePreview) {
    let mock = Mock::new(move |request| {
        let path = request.path.as_str();
        if path.ends_with("/desktop/capabilities") {
            (200, r#"{"features":["issueMoves"]}"#.into())
        } else if request.method == "PATCH" && path.ends_with(&format!("/issues/{ROOT}")) {
            // NETT moved the whole subtree and returns the root there.
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
        Some(subspace_framework::core::Secret::new("nett-move-reconcile")),
    )
    .unwrap()
    .without_live();
    (mock, client, plan(children))
}

/// The previewed plan: ROOT, then each of `children`, from OPS to WEB.
pub(super) fn plan(children: &[(&str, &str)]) -> MovePreview {
    let issues: Vec<Value> = std::iter::once(item(ROOT, "OPS-2"))
        .chain(children.iter().map(|(id, old)| item(id, old)))
        .collect();
    serde_json::from_value(json!({
        "token": "a".repeat(64),
        "source": {"id": "team-ops", "key": "OPS", "name": "Operations"},
        "destination": {"id": "team-web", "key": "WEB", "name": "Web"},
        "issues": issues,
        "reviewContext": {"snapshot": 1},
    }))
    .unwrap()
}

/// Confirms the move with CHILD read back as `child` answers.
fn committed_move(child: impl Fn() -> (u16, String) + Send + Sync + 'static) -> MoveResult {
    let (mock, client, preview) = committing(&[(CHILD, "OPS-3")], child);
    let result = client
        .move_issue(&preview)
        .unwrap_or_else(|error| panic!("the move committed, not {error:?}"));
    assert_eq!(patches(&mock), 1, "the move is never sent again");
    result
}

pub(super) fn patches(mock: &Mock) -> usize {
    mock.requests()
        .iter()
        .filter(|request| request.method == "PATCH")
        .count()
}

pub(super) fn unreconciled(reason: UnreconciledReason) -> [Unreconciled; 1] {
    [Unreconciled::new(CHILD, "OPS-3", reason)]
}

#[test]
fn a_committed_move_stays_committed_when_a_sub_issue_moves_again_before_it_is_read() {
    // Someone else moved the sub-issue on, to a third team, before it was
    // read.
    let child = || {
        (
            200,
            issue(CHILD, "DOC-9", ("team-docs", "DOC"), 4).to_string(),
        )
    };
    let result = committed_move(child);
    assert_eq!(result.issue.fields.identifier, "WEB-7");
    assert_eq!(
        result.unreconciled,
        unreconciled(UnreconciledReason::Elsewhere)
    );
    assert_eq!(result.moved.len(), 1);
}

#[test]
fn a_stale_sub_issue_read_is_never_taken_for_another_move() {
    // A read that still shows the sub-issue as previewed, or back where it
    // was, shows nothing about a later move.
    for (team, version) in [(("team-ops", "OPS"), 2), (("team-ops", "OPS"), 5)] {
        let result =
            committed_move(move || (200, issue(CHILD, "OPS-3", team, version).to_string()));
        assert_eq!(
            result.unreconciled,
            unreconciled(UnreconciledReason::NotConfirmed)
        );
    }
}

#[test]
fn a_committed_move_keeps_its_proof_when_a_sub_issue_cannot_be_read_back() {
    // Moved on at once into a team the person cannot read.
    let result = committed_move(|| (404, "{}".into()));
    assert_eq!(
        result.issue.fields.identifier, "WEB-7",
        "the move committed"
    );
    assert_eq!(
        result.unreconciled,
        unreconciled(UnreconciledReason::Unread(Error::NotFound)),
        "the sub-issue is listed with what its read found"
    );
}

#[test]
fn a_move_read_back_as_the_account_changes_returns_nothing_of_the_old_one() {
    let (mock, mut client, preview) = committing(&[(CHILD, "OPS-3"), (SECOND, "OPS-4")], || {
        (
            200,
            issue(CHILD, "WEB-8", ("team-web", "WEB"), 3).to_string(),
        )
    });
    // Bound to account 1's lease, as a native client is, with what NETT
    // serves already read.
    test_lease::set(Some(1));
    test_lease::set_account(Some(1));
    client.native_lease = Some(crate::NativeLease(Some(1)));
    assert!(client.serves(crate::backend::Feature::IssueMoves));
    // Another account signs in as the first sub-issue is read back.
    test_lease::AFTER_READ.with(|next| next.set(Some(2)));
    let result = client.move_issue(&preview);
    test_lease::set(None);
    test_lease::set_account(None);
    assert_eq!(result.err(), Some(IssueSaveError::SignIn));
    let reads = mock
        .requests()
        .iter()
        .filter(|request| request.method == "GET" && request.path.contains("/issues/"))
        .count();
    assert_eq!(
        reads, 1,
        "the check stops at the read that found the change"
    );
    assert_eq!(patches(&mock), 1);
}

#[test]
fn reading_a_committed_move_back_takes_at_most_its_budget() {
    // The connection fails after the PATCH: every read would hang.
    let (mock, client, preview) = committing(&[(CHILD, "OPS-3"), (SECOND, "OPS-4")], || {
        std::thread::sleep(Duration::from_millis(1500));
        (200, "{}".into())
    });
    let root = serde_json::from_value(issue(ROOT, "WEB-7", ("team-web", "WEB"), 3)).unwrap();
    let started = Instant::now();
    let result = client
        .nett_verify_move_within(&preview, Some(root), Duration::from_millis(300))
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(1200),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(
        result.unreconciled,
        [
            Unreconciled::new(CHILD, "OPS-3", UnreconciledReason::NotChecked),
            Unreconciled::new(SECOND, "OPS-4", UnreconciledReason::NotChecked),
        ],
        "the read cut short and the one never read are both listed"
    );
    assert_eq!(patches(&mock), 0, "a read back sends no move");
}

#[test]
fn waiting_for_a_place_among_the_accounts_requests_counts_against_the_budget() {
    let (mock, client, preview) = committing(&[(CHILD, "OPS-3"), (SECOND, "OPS-4")], || {
        (
            200,
            issue(CHILD, "WEB-8", ("team-web", "WEB"), 3).to_string(),
        )
    });
    // Every place among the account's requests is taken, by reads that
    // end only well after the budget.
    let account = client.flow_account();
    let taken: Vec<_> = (0..crate::nett::flow::CONCURRENT)
        .map(|_| crate::nett::flow::acquire(account).unwrap())
        .collect();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        drop(taken);
    });
    let root = serde_json::from_value(issue(ROOT, "WEB-7", ("team-web", "WEB"), 3)).unwrap();
    let started = Instant::now();
    let result = client
        .nett_verify_move_within(&preview, Some(root), Duration::from_millis(300))
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(1200),
        "took {:?}",
        started.elapsed()
    );
    assert!(
        result
            .unreconciled
            .iter()
            .all(|item| item.reason == UnreconciledReason::NotChecked),
        "{:?}",
        result.unreconciled
    );
    let reads = mock
        .requests()
        .iter()
        .filter(|request| request.path.contains("/issues/"))
        .count();
    assert_eq!(reads, 0, "no read went out after its time");
    release.join().unwrap();
}

#[test]
fn a_reply_about_another_issue_never_shows_a_sub_issue_moved_again() {
    // A reply that names another issue, newer and in a third team.
    let child = || {
        (
            200,
            issue(SECOND, "DOC-9", ("team-docs", "DOC"), 4).to_string(),
        )
    };
    let result = committed_move(child);
    assert_eq!(
        result.unreconciled,
        unreconciled(UnreconciledReason::NotConfirmed)
    );
}
