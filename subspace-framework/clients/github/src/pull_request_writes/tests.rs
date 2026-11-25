use serde_json::{Value, json};
use subspace_client_test_support::Mock;

use crate::tests::{client, token};

#[test]
fn rest_comment_and_close_use_selected_account_and_exact_bodies() {
    let mock = Mock::new(|request| {
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| key == "authorization" && value == "Bearer selected")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| key == "x-github-api-version" && value == "2022-11-28")
        );
        (200, "{}".into())
    });
    let client = client(mock.base_url().into(), &token("selected"));
    assert!(
        client
            .issue_comment("ann", "o/r#42", "  Closing this.\nThanks.  ")
            .is_ok()
    );
    assert!(client.close_pull_request("ann", "o/r#42").is_ok());
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        (&*requests[0].method, &*requests[0].path),
        ("POST", "/repos/o/r/issues/42/comments")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].body).unwrap(),
        json!({"body": "Closing this.\nThanks."})
    );
    assert_eq!(
        (&*requests[1].method, &*requests[1].path),
        ("PATCH", "/repos/o/r/pulls/42")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&requests[1].body).unwrap(),
        json!({"state":"closed"})
    );
}

#[test]
fn rejected_writes_are_mapped_and_never_retried() {
    for status in [400, 401, 403, 422, 500] {
        let mock = Mock::new(move |_| {
            (
                status,
                json!({"message":"private response details"}).to_string(),
            )
        });
        let client = client(mock.base_url().into(), &token("selected"));
        assert!(client.issue_comment("ann", "o/r#42", "Comment").is_err());
        assert!(client.close_pull_request("ann", "o/r#42").is_err());
        assert_eq!(mock.requests().len(), 2, "HTTP {status} must not retry");
    }
}

#[test]
fn invalid_inputs_make_no_requests() {
    let mock = Mock::new(|_| panic!("invalid inputs must not reach GitHub"));
    let client = client(mock.base_url().into(), &token("selected"));
    assert!(
        client
            .close_pull_request("invalid account", "o/r#42")
            .is_err()
    );
    assert!(client.close_pull_request("ann", "bad/key#0").is_err());
    assert!(client.issue_comment("ann", "o/r#42", "  ").is_err());
    assert!(
        client
            .issue_comment("ann", "o/r#42", &"x".repeat(65537))
            .is_err()
    );
    assert!(mock.requests().is_empty());
}

#[test]
fn detail_offers_merge_only_with_current_merge_state_permission_and_method() {
    use crate::model::{MergeMethod, MergeTarget};

    for (state, git_mergeable, permission, draft, method, enabled) in [
        ("CLEAN", "MERGEABLE", "WRITE", false, true, true),
        ("UNSTABLE", "MERGEABLE", "MAINTAIN", false, true, true),
        ("BLOCKED", "MERGEABLE", "WRITE", false, true, false),
        ("BEHIND", "MERGEABLE", "ADMIN", false, true, false),
        ("DIRTY", "CONFLICTING", "ADMIN", false, true, false),
        ("UNKNOWN", "UNKNOWN", "ADMIN", false, true, false),
        ("CLEAN", "MERGEABLE", "READ", false, true, false),
        ("CLEAN", "MERGEABLE", "WRITE", true, true, false),
        ("CLEAN", "MERGEABLE", "WRITE", false, false, false),
    ] {
        let mock = Mock::new(move |request| {
            for field in [
                "mergeable",
                "mergeStateStatus",
                "headRefOid",
                "viewerPermission",
            ] {
                assert!(request.body.contains(field));
            }
            let mut pr = crate::tests::pr("PR_fixture", 42);
            pr["bodyHTML"] = json!("<p>Example</p>");
            pr["baseRefName"] = json!("main");
            pr["isDraft"] = json!(draft);
            pr["mergeable"] = json!(git_mergeable);
            pr["mergeStateStatus"] = json!(state);
            pr["headRefOid"] = json!("a".repeat(40));
            (
                200,
                json!({"data":{"repository":{
                    "viewerPermission": permission, "squashMergeAllowed": method,
                    "issueOrPullRequest": pr
                }}})
                .to_string(),
            )
        });
        let client = client(mock.base_url().into(), &token("selected"));
        let detail = client.item("ann", "o/r#42").unwrap();
        let target = enabled.then(|| MergeTarget {
            head_sha: "a".repeat(40),
            base_ref: "main".into(),
            method: MergeMethod::Squash,
        });
        assert_eq!(detail.merge_target, target, "{state}/{permission}");
        assert!(
            serde_json::to_value(detail)
                .unwrap()
                .get("mergeTarget")
                .is_none()
        );
    }
}

#[test]
fn merge_is_single_attempt_with_the_confirmed_sha_and_selected_account() {
    use crate::model::{MergeMethod, MergeTarget};

    for (status, merged) in [(200, true), (200, false), (409, false)] {
        let mock = Mock::new(move |request| {
            if request.method == "PUT" {
                return (status, json!({"merged": merged}).to_string());
            }
            merge_preflight(&request.body, "ann", "main")
        });
        let client = client(mock.base_url().into(), &token("selected"));
        let target = MergeTarget {
            head_sha: "a".repeat(40),
            base_ref: "main".into(),
            method: MergeMethod::Rebase,
        };
        assert_eq!(
            client.merge_pull_request("ann", "o/r#42", &target).is_ok(),
            merged
        );
        let requests = mock.requests();
        assert_eq!(requests.len(), 3);
        let request = &requests[2];
        assert_eq!(
            (&*request.method, &*request.path),
            ("PUT", "/repos/o/r/pulls/42/merge")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "authorization" && value == "Bearer selected")
        );
        assert_eq!(
            serde_json::from_str::<Value>(&request.body).unwrap(),
            json!({"sha": target.head_sha, "merge_method": "rebase"})
        );
    }
}

fn merge_preflight(body: &str, viewer: &str, base: &str) -> (u16, String) {
    if body.contains("ViewerBound") {
        return (
            200,
            json!({"data":{"viewer":{"id":"fixture-viewer","login":viewer}}}).to_string(),
        );
    }
    let mut pr = crate::tests::pr("PR_fixture", 42);
    pr["bodyHTML"] = json!("");
    pr["baseRefName"] = json!(base);
    pr["mergeable"] = json!("MERGEABLE");
    pr["mergeStateStatus"] = json!("CLEAN");
    pr["headRefOid"] = json!("a".repeat(40));
    (
        200,
        json!({"data":{"repository":{
            "viewerPermission": "WRITE", "rebaseMergeAllowed": true, "issueOrPullRequest": pr
        }}})
        .to_string(),
    )
}

#[test]
fn merge_refuses_a_different_token_owner_or_base_before_sending_a_write() {
    use crate::model::{MergeMethod, MergeTarget};
    let target = MergeTarget {
        head_sha: "a".repeat(40),
        base_ref: "main".into(),
        method: MergeMethod::Rebase,
    };
    for (viewer, base) in [("another-viewer", "main"), ("ann", "other-branch")] {
        let mock = Mock::new(move |request| {
            assert_ne!(request.method, "PUT");
            merge_preflight(&request.body, viewer, base)
        });
        let client = client(mock.base_url().into(), &token("selected"));
        assert!(client.merge_pull_request("ann", "o/r#42", &target).is_err());
        assert!(
            mock.requests()
                .iter()
                .all(|request| request.method != "PUT")
        );
    }
}

#[test]
fn review_verifies_identity_and_uses_one_credential_for_the_write_and_refresh() {
    use crate::model::ReviewEvent;

    let credentials = token("selected");
    let environment = credentials.clone();
    let mock = Mock::new(move |request| {
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| { name == "authorization" && value == "Bearer selected" })
        );
        let body = serde_json::from_str::<Value>(&request.body).unwrap();
        let query = body["query"].as_str().unwrap();
        let data = if query.contains("ViewerBound") {
            *environment.lock().unwrap() = "replacement".into();
            json!({"viewer":{"id":"fixture-viewer","login":"ann"}})
        } else if query.contains("query Subject") {
            json!({"repository":{"issueOrPullRequest":{"__typename":"PullRequest","id":"PR_fixture"}}})
        } else if query.contains("mutation Review") {
            assert_eq!(
                body["variables"],
                json!({"id":"PR_fixture","event":"APPROVE","body":null})
            );
            json!({"addPullRequestReview":{"pullRequestReview":{"id":"review-fixture"}}})
        } else {
            let mut pr = crate::tests::pr("PR_fixture", 42);
            pr["bodyHTML"] = json!("");
            pr["viewerLatestReview"] = json!({"state":"APPROVED","author":{"login":"ann"}});
            json!({"repository":{"issueOrPullRequest":pr}})
        };
        (200, json!({"data":data}).to_string())
    });
    let client = client(mock.base_url().into(), &credentials);

    let detail = client
        .review("ann", "o/r#42", ReviewEvent::Approve, "")
        .unwrap();
    assert!(detail.viewer_approved);
    assert_eq!(mock.requests().len(), 4);

    *credentials.lock().unwrap() = "selected".into();
    let error = client
        .review("bob", "o/r#42", ReviewEvent::Approve, "")
        .unwrap_err();
    assert!(matches!(
        error,
        subspace_core::http::Error::Status { status: 401, .. }
    ));
    assert_eq!(
        mock.requests().len(),
        5,
        "only verify the mismatched viewer"
    );

    *credentials.lock().unwrap() = "selected".into();
    assert!(!client.item("bob", "o/r#42").unwrap().viewer_approved);
}
