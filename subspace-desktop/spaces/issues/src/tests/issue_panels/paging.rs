use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::{has, label, opened, render};
use crate::tests::offline::{Offline, offline_with, settle_board};

fn pull_request_continuation_failure() -> Offline {
    offline_with(|request| {
        if !request.path.contains("/pull-requests?") {
            return None;
        }
        if request.path.contains("cursor=") {
            return Some((500, "{}".into()));
        }
        Some((200, json!({"items":[{
            "id":"retained-pr", "repository":{"owner":"octo-org","name":"api","fullName":"octo-org/api","url":"https://github.com/octo-org/api"},
            "number":421,"url":"https://github.com/octo-org/api/pull/421",
            "title":"Existing pull request","state":"open","isDraft":false,
            "headBranch":"fixture","baseBranch":"main","referenceKind":"closes","referenceSources":[],
            "referenceActive":true,"completion":"pending","githubUpdatedAt":"2026-11-01T00:00:00Z","lastObservedAt":"2026-11-01T00:00:00Z"
        }],"nextCursor":"prs-next"}).to_string()))
    })
}

#[gpui_kit::test]
fn a_failed_pull_request_continuation_keeps_rows_and_names_the_failure(cx: &mut TestAppContext) {
    let fixture = pull_request_continuation_failure();
    let (board, cx) = opened(&fixture, cx);
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| s.pull_requests().is_some_and(|p| !p.items.is_empty()))
    });
    render(cx);
    let row = "issue-pr:retained-pr";
    let original = label(cx, row);
    assert!(original.is_some());
    cx.update(|window, cx| window.click("issue-more-pull-requests", cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .pull_requests_failed()
    });
    render(cx);
    assert_eq!(
        label(cx, "issue-more-pull-requests-error").as_deref(),
        Some("Couldn't load more pull requests")
    );
    assert_eq!(label(cx, row), original);
    assert_eq!(
        label(cx, "issue-more-pull-requests").as_deref(),
        Some("Retry pull requests")
    );
    assert!(has(cx, "issue-title"));
}

pub(super) fn assert_first_page_failure(cx: &mut VisualTestContext, relations: bool) {
    let (id, message) = if relations {
        ("issue-more-relations-error", "Couldn't load relations")
    } else {
        (
            "issue-more-pull-requests-error",
            "Couldn't load pull requests",
        )
    };
    assert_eq!(label(cx, id).as_deref(), Some(message));
}
