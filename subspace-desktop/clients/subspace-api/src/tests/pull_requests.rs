use super::{client, common, connected, page, wait};
use crate::{PageRequest, hub::IssueStatus, model::IssueListItem};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::Mock;

pub(crate) fn pull_request(id: &str) -> Value {
    json!({"id":id,"repository":{"owner":"fixture","name":"workspace","fullName":"fixture/workspace","url":"https://github.com/fixture/workspace"},
        "number":1,"url":"https://github.com/fixture/workspace/pull/1","title":"Closes SUB-1","authorLogin":"octo-dev",
        "state":"merged","isDraft":false,"headBranch":"fixture/SUB-1","baseBranch":"main","referenceKind":"closes",
        "referenceSources":["title","branch"],"referenceActive":true,"completion":"completed","mergedAt":"2026-11-16T12:00:00Z",
        "closedAt":"2026-11-16T12:00:00Z","githubUpdatedAt":"2026-11-16T12:00:00Z","lastObservedAt":"2026-11-16T12:01:00Z"})
}

#[test]
fn summary_is_optional_and_all_contract_fields_deserialize() {
    let mut value = super::row("1");
    let row: IssueListItem = serde_json::from_value(value.clone()).unwrap();
    assert!(row.pull_request_summary.is_none());
    value["pullRequestSummary"] = json!({"total":3,"open":1,"closed":1,"merged":1,"closingPending":1,"lastObservedAt":"2026-11-16T12:01:00Z"});
    let row: IssueListItem = serde_json::from_value(value).unwrap();
    assert_eq!(
        row.pull_request_summary.as_ref().unwrap().closing_pending,
        1
    );
    let pr: crate::model::PullRequest = serde_json::from_value(pull_request("pr-1")).unwrap();
    assert_eq!(pr.repository.full_name, "fixture/workspace");
    assert_eq!(pr.completion, "completed");
    assert_eq!(pr.reference_sources, ["title", "branch"]);
}

#[test]
fn ready_for_agents_is_read_when_sent_and_unknown_when_not() {
    let mut value = super::row("1");
    let row: IssueListItem = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(row.ready_for_agents, None, "an older service sends no flag");
    for flag in [true, false] {
        value["readyForAgents"] = json!(flag);
        let row: IssueListItem = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(row.ready_for_agents, Some(flag));
    }
}

#[test]
fn route_404_is_neutral_and_does_not_hide_the_issue() {
    let mock = Mock::new(common);
    let hub = connected(&mock);
    let snapshot = wait(&hub, |s| {
        s.issue().is_some_and(|i| i.pull_requests_unavailable())
    });
    let issue = snapshot.issue().unwrap();
    assert_eq!(issue.status(), &IssueStatus::Loaded);
    assert!(issue.detail().is_some());
    assert!(!issue.pull_requests_failed());
    assert!(issue.pull_requests().is_none());
    hub.stop();
}

#[test]
fn independent_pages_retry_preserve_rows_and_deduplicate() {
    let fail = Arc::new(AtomicBool::new(true));
    let switch = fail.clone();
    let mock = Mock::new(move |r| {
        if r.path.contains("/pull-requests?") {
            return if !r.path.contains("cursor=") {
                (200, page(vec![pull_request("first")], Some("next")))
            } else if switch.load(Ordering::Relaxed) {
                (500, "{}".into())
            } else {
                (
                    200,
                    page(vec![pull_request("first"), pull_request("second")], None),
                )
            };
        }
        common(r)
    });
    let hub = connected(&mock);
    wait(&hub, |s| {
        s.issue().is_some_and(|i| i.pull_requests().is_some())
    });
    hub.load_more_pull_requests().unwrap();
    let failed = wait(&hub, |s| s.issue().unwrap().pull_requests_failed());
    assert_eq!(
        failed.issue().unwrap().pull_requests().unwrap().items.len(),
        1
    );
    assert!(!failed.issue().unwrap().relations_failed());
    fail.store(false, Ordering::Relaxed);
    hub.load_more_pull_requests().unwrap();
    let recovered = wait(&hub, |s| {
        s.issue()
            .unwrap()
            .pull_requests()
            .is_some_and(|p| p.next_cursor.is_none())
    });
    assert_eq!(
        recovered
            .issue()
            .unwrap()
            .pull_requests()
            .unwrap()
            .items
            .len(),
        2
    );
    assert!(
        mock.requests()
            .iter()
            .any(|r| r.path == "/api/v1/issues/SUB-1/pull-requests?limit=20&cursor=next")
    );
    hub.stop();
}

#[test]
fn failed_first_page_retries_and_page_limits_are_bounded() {
    let fail = Arc::new(AtomicBool::new(true));
    let switch = fail.clone();
    let mock = Mock::new(move |r| {
        if r.path.contains("/pull-requests?") {
            return if switch.load(Ordering::Relaxed) {
                (500, "{}".into())
            } else {
                (200, page(vec![pull_request("first")], None))
            };
        }
        common(r)
    });
    let hub = connected(&mock);
    wait(&hub, |s| s.issue().unwrap().pull_requests_failed());
    fail.store(false, Ordering::Relaxed);
    hub.load_more_pull_requests().unwrap();
    let loaded = wait(&hub, |s| s.issue().unwrap().pull_requests().is_some());
    assert_eq!(
        loaded.issue().unwrap().pull_requests().unwrap().items.len(),
        1
    );
    for limit in [0, 101] {
        let request = PageRequest {
            limit,
            cursor: None,
        };
        assert!(matches!(
            client(&mock).pull_requests("SUB-1", &request),
            Err(crate::Error::InvalidRequest)
        ));
    }
    hub.stop();
}

#[test]
fn opening_from_all_projects_demands_only_the_issues_workflow() {
    let mock = Mock::new(common);
    let hub = connected(&mock);
    let snapshot = hub.snapshot();
    assert!(!snapshot.projects()[0].states.is_empty());
    hub.reload_issue().unwrap();
    wait(&hub, |s| {
        s.issue().unwrap().status() == &IssueStatus::Loaded
    });
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.path.contains("workflow-states"))
            .count(),
        1
    );
    assert!(
        hub.snapshot().projects()[0]
            .states
            .iter()
            .any(|s| s.id == "state-1")
    );
    hub.stop();
}
