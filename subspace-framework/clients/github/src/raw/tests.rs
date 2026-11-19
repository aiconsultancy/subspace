use serde_json::{Value, json};

use super::activity::check_state;
use super::projection::to_item;
use super::types::{RawCommon, RawContext};
use super::{RawPrFile, extras, section_items, to_detail, to_diff_file};
use crate::model::{
    CheckRunState, CheckState, EventKind, GhFileStatus, PrState, ReviewDecision, ReviewerState,
};

fn pr_node() -> Value {
    json!({
        "__typename": "PullRequest", "id": "PR_1", "number": 7, "title": "Add rows",
        "url": "https://github.com/octo/app/pull/7", "state": "OPEN",
        "createdAt": "2026-09-01T10:00:00Z", "updatedAt": "2026-09-02T10:00:00Z",
        "repository": {"nameWithOwner": "octo/app"}, "author": {"login": "ann_emu", "name": ""},
        "assignees": {"nodes": [null, {"login": "bob"}]},
        "labels": {"nodes": [{"name": "ui", "color": "a2eeef"}]}, "comments": {"totalCount": 3},
        "isDraft": true, "headRefName": "rows", "baseRefName": "main",
        "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": "ERROR"}}}]}
    })
}

fn detail() -> crate::model::GhItemDetail {
    let mut node = pr_node();
    node.as_object_mut().unwrap().extend(json!({
        "bodyHTML": "<p>hi</p>", "viewerDidAuthor": false, "viewerCanReact": true,
        "locked": false, "changedFiles": 4, "reviewDecision": "APPROVED",
        "latestReviews": {"nodes": [{"author": {"login": "Carl"}, "state": "COMMENTED"}, {"author": {"login": "dee"}, "state": "APPROVED"}]},
        "latestOpinionatedReviews": {"nodes": [{"author": {"login": "carl"}, "state": "CHANGES_REQUESTED"}]},
        "reviewRequests": {"nodes": [{"requestedReviewer": {"__typename": "Team", "combinedSlug": "octo/core", "name": "Core"}}, {"requestedReviewer": {"__typename": "Bot", "login": "robot"}}]},
        "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": "PENDING", "contexts": {"nodes": [
            {"__typename": "CheckRun", "name": "build", "status": "COMPLETED", "conclusion": "TIMED_OUT", "detailsUrl": "https://ci/1"},
            {"__typename": "CheckRun", "name": "lint", "status": "IN_PROGRESS", "conclusion": null, "detailsUrl": null},
            {"__typename": "StatusContext", "context": "legacy", "state": "SUCCESS", "targetUrl": null}
        ]}}}}]},
        "commentEvents": {"nodes": [{"author": null, "createdAt": "2026-09-03T00:00:00Z", "bodyHTML": "c", "url": "https://c"}]},
        "reviews": {"nodes": [
            {"author": {"login": "dee"}, "state": "APPROVED", "createdAt": "2026-09-01T00:00:00Z", "submittedAt": "2026-09-04T00:00:00Z", "bodyHTML": "", "body": "", "url": "https://r"},
            {"author": {"login": "carl"}, "state": "COMMENTED", "createdAt": "2026-09-05T00:00:00Z", "body": "  "},
            {"author": {"login": "eve"}, "state": "PENDING", "createdAt": "2026-09-06T00:00:00Z", "body": "draft"}
        ]}
    }).as_object().unwrap().clone());
    to_detail(&serde_json::from_value(node).unwrap())
}

#[test]
fn section_rows_merge_extras_and_drop_null_nodes() {
    let items = section_items(
        &[pr_node(), Value::Null, json!({})],
        &extras(&[
            json!({"id": "PR_1", "reviewDecision": "CHANGES_REQUESTED", "additions": 5, "deletions": 1}),
        ]),
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, PrState::Draft);
    assert_eq!(items[0].checks, Some(CheckState::Failure));
    assert_eq!(
        items[0].review_decision,
        Some(ReviewDecision::ChangesRequested)
    );
}

#[test]
fn mapped_item_owns_fields_from_borrowed_source() {
    let mut raw: RawCommon = serde_json::from_value(pr_node()).unwrap();
    let item = to_item(&raw);

    raw.title = "Changed title".into();
    raw.url = "changed-url".into();
    raw.labels.as_mut().unwrap().nodes[0].as_mut().unwrap().name = "changed-label".into();

    assert_eq!(item.title, "Add rows");
    assert_eq!(item.url, "https://github.com/octo/app/pull/7");
    assert_eq!(item.labels[0].name, "ui");
}

#[test]
fn detail_maps_reviewers() {
    let detail = detail();
    let reviewers: Vec<(&str, ReviewerState)> = detail
        .reviewers
        .iter()
        .map(|r| (r.user.login.as_str(), r.state))
        .collect();
    assert_eq!(
        reviewers,
        [
            ("carl", ReviewerState::ChangesRequested),
            ("dee", ReviewerState::Approved),
            ("octo/core", ReviewerState::Pending),
            ("robot", ReviewerState::Pending)
        ]
    );
}

#[test]
fn detail_maps_checks_and_activity() {
    let detail = detail();
    assert!(detail.viewer_can_review);
    assert!(detail.viewer_can_comment);
    assert_eq!(detail.changed_files, Some(4));
    assert_eq!(
        detail
            .check_runs
            .iter()
            .map(|c| c.state)
            .collect::<Vec<_>>(),
        [
            CheckRunState::Failure,
            CheckRunState::Pending,
            CheckRunState::Success
        ]
    );
    assert_eq!(detail.events.len(), 2);
    assert_eq!(detail.events[0].kind, EventKind::Review);
    assert_eq!(detail.events[0].created_at, "2026-09-04T00:00:00Z");
    assert_eq!(detail.events[1].kind, EventKind::Comment);
}

#[test]
fn issues_have_no_review_fields() {
    let node = json!({"__typename": "Issue", "id": "I", "number": 1, "title": "t", "url": "u", "state": "OPEN", "createdAt": "a", "updatedAt": "b", "repository": {"nameWithOwner": "o/r"}, "author": null, "assignees": {"nodes": []}, "labels": {"nodes": []}, "comments": {"totalCount": 0}, "bodyHTML": "", "viewerCanReact": true, "locked": true});
    let detail = to_detail(&serde_json::from_value(node).unwrap());
    assert!(!detail.viewer_can_review);
    assert!(detail.reviewers.is_empty());
    assert_eq!(detail.changed_files, None);
}

#[test]
fn check_run_conclusions() {
    let run = |status: &str, conclusion: Option<&str>| RawContext::CheckRun {
        name: "x".into(),
        status: status.into(),
        conclusion: conclusion.map(Into::into),
        details_url: None,
    };
    assert_eq!(
        check_state(&run("COMPLETED", Some("STALE"))),
        CheckRunState::Neutral
    );
    assert_eq!(
        check_state(&run("COMPLETED", Some("SKIPPED"))),
        CheckRunState::Skipped
    );
    assert_eq!(check_state(&run("COMPLETED", None)), CheckRunState::Failure);
    assert_eq!(check_state(&run("QUEUED", None)), CheckRunState::Pending);
}

#[test]
fn diff_files_default_unknown_status_and_empty_patch() {
    let raw: RawPrFile = serde_json::from_value(
        json!({"filename": "b.rs", "previous_filename": "a.rs", "status": "weird", "patch": ""}),
    )
    .unwrap();
    let file = to_diff_file(raw);
    assert_eq!(file.status, GhFileStatus::Modified);
    assert_eq!(file.previous_path.as_deref(), Some("a.rs"));
    assert_eq!(file.patch, None);
    assert_eq!((file.additions, file.deletions), (0, 0));
}
