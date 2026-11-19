use super::{client, comment, common, connected, page, wait};
use crate::{CommentIntent, CommentPost, WriteError, hub::CommentState};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use subspace_framework::client_test_support::{Mock, Request};
mod recovery;

fn post_count(mock: &Mock) -> usize {
    mock.requests()
        .iter()
        .filter(|request| request.method == "POST")
        .count()
}

#[test]
fn root_and_reply_posts_send_only_body_and_parent_without_idempotency_tokens() {
    let mock = Mock::new(|request| {
        let input: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        (
            201,
            comment(
                "comment-1",
                input["body"].as_str().unwrap(),
                input["parentCommentId"].as_str(),
            )
            .to_string(),
        )
    });
    let api = client(&mock);
    for parent in [None, Some("root-1")] {
        let intent = CommentIntent::new("Fixture text", parent).unwrap();
        assert!(matches!(
            api.post_comment("SUB-1", &intent),
            Ok(CommentPost::Created(_))
        ));
    }
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&requests[1].body).unwrap(),
        json!({"body":"Fixture text","parentCommentId":"root-1"})
    );
    for request in requests {
        assert_eq!(request.path, "/api/v1/issues/SUB-1/comments");
        assert!(!request.headers.iter().any(|(key, _)| matches!(
            key.as_str(),
            "authorization" | "x-org-id" | "idempotency-key"
        )));
    }
}

#[test]
fn successful_post_clears_draft_and_projects_comment() {
    let mock = Mock::new(|request| {
        if request.method == "POST" {
            (201, comment("comment-1", "Fixture text", None).to_string())
        } else {
            common(request)
        }
    });
    let hub = connected(&mock);
    hub.edit_comment("1", None, "Fixture text").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().confirmation() == 1
    });
    assert_eq!(hub.comment_draft("1", None).unwrap().body(), "");
    assert_eq!(
        hub.snapshot().issue().unwrap().timeline().projected().len(),
        1
    );
    assert_eq!(post_count(&mock), 1);
    hub.stop();
}

#[test]
fn lost_response_freezes_draft_and_complete_paged_check_finds_saved_comment() {
    let saved = Arc::new(Mutex::new(None));
    let storage = saved.clone();
    let mock = Mock::new(move |request| {
        if request.method == "POST" {
            *storage.lock().unwrap() = Some(comment("saved", "Fixture text", None));
            return (0, String::new());
        }
        if request.path.starts_with("/api/v1/issues/1/comments?") {
            let items = if request.path.contains("cursor=") {
                storage.lock().unwrap().clone().into_iter().collect()
            } else {
                vec![]
            };
            let cursor = (!request.path.contains("cursor=")).then_some("second-page");
            return (200, page(items, cursor));
        }
        common(request)
    });
    let hub = connected(&mock);
    hub.edit_comment("1", None, "Fixture text").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", None).unwrap().state(),
            CommentState::Unclear { .. }
        )
    });
    hub.edit_comment("1", None, "must stay frozen").unwrap();
    hub.submit_comment("1", None, true).unwrap();
    assert_eq!(hub.comment_draft("1", None).unwrap().body(), "Fixture text");
    assert_eq!(post_count(&mock), 1);
    hub.check_comment("1", None).unwrap();
    wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().confirmation() == 1
    });
    assert!(hub.comment_draft("1", None).unwrap().body().is_empty());
    assert_eq!(post_count(&mock), 1);
    assert!(
        mock.requests()
            .iter()
            .any(|request| request.path.contains("cursor=second-page"))
    );
    hub.stop();
}

#[test]
fn absent_comment_enables_separate_explicit_post_again_with_duplicate_warning() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let posts = attempts.clone();
    let mock = Mock::new(move |request| {
        if request.method == "POST" {
            return if posts.fetch_add(1, Ordering::SeqCst) == 0 {
                (503, "opaque failure".into())
            } else {
                (201, comment("second", "Fixture text", None).to_string())
            };
        }
        if request.path.starts_with("/api/v1/issues/1/comments?") {
            return (200, page(vec![], None));
        }
        common(request)
    });
    let hub = connected(&mock);
    hub.edit_comment("1", None, "Fixture text").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", None).unwrap().state(),
            CommentState::Unclear { .. }
        )
    });
    hub.check_discussion("1", None).unwrap();
    wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", None).unwrap().state(),
            CommentState::Unclear { refresh: true }
        )
    });
    assert!(
        hub.comment_draft("1", None)
            .unwrap()
            .notice()
            .unwrap()
            .contains("twice")
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    hub.submit_comment("1", None, false).unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    hub.submit_comment("1", None, true).unwrap();
    wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().confirmation() == 1
    });
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    hub.stop();
}

#[test]
fn reply_check_uses_replies_and_rejects_other_author_parent_body_and_old_comments() {
    let mock = Mock::new(|request| {
        if request.method == "POST" {
            return (500, "opaque failure".into());
        }
        if request
            .path
            .starts_with("/api/v1/issues/1/comments/root-1/replies?")
        {
            let mut other_author = comment("author", "Fixture text", Some("root-1"));
            other_author["actualAuthor"]["userId"] = json!("other");
            let mut old = comment("old", "Fixture text", Some("root-1"));
            old["createdAt"] = json!("2020-01-01T00:00:00Z");
            return (
                200,
                page(
                    vec![
                        other_author,
                        old,
                        comment("body", "other text", Some("root-1")),
                        comment("parent", "Fixture text", Some("other-root")),
                    ],
                    None,
                ),
            );
        }
        common(request)
    });
    let hub = connected(&mock);
    hub.activate_timeline(0).unwrap();
    wait(&hub, |snapshot| {
        snapshot.issue().unwrap().timeline().page().is_some()
    });
    hub.edit_comment("1", Some("root-1"), "Fixture text")
        .unwrap();
    hub.submit_comment("1", Some("root-1"), false).unwrap();
    wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", Some("root-1")).unwrap().state(),
            CommentState::Unclear { .. }
        )
    });
    hub.check_comment("1", Some("root-1")).unwrap();
    wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", Some("root-1")).unwrap().state(),
            CommentState::Unclear { refresh: true }
        )
    });
    assert_eq!(post_count(&mock), 1);
    assert_eq!(
        hub.comment_draft("1", Some("root-1"))
            .unwrap()
            .confirmation(),
        0
    );
    hub.stop();
}

#[test]
fn definite_refusals_preserve_draft_and_never_replay_post() {
    let mock = Mock::with_headers(|request| {
        if request.method == "POST" {
            (
                403,
                vec![("Content-Type".into(), "application/problem+json".into())],
                "{}".into(),
            )
        } else {
            let (status, body) = common(request);
            (status, vec![], body)
        }
    });
    let hub = connected(&mock);
    hub.edit_comment("1", None, "Fixture text").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", None).unwrap().state(),
            CommentState::Failed(WriteError::Refused { status: 403, .. })
        )
    });
    assert_eq!(post_count(&mock), 1);
    assert_eq!(hub.comment_draft("1", None).unwrap().body(), "Fixture text");
    hub.stop();
}

#[test]
fn oversized_empty_and_nul_comments_are_rejected_before_network() {
    assert!(CommentIntent::new(" ", None).is_err());
    assert!(CommentIntent::new("nul\0text", None).is_err());
    assert_eq!(
        CommentIntent::new("line\r\nnext\rlast", None)
            .unwrap()
            .body(),
        "line\nnext\nlast"
    );
    assert!(CommentIntent::new(&"a".repeat(50_001), None).is_err());
    assert!(CommentIntent::new("Fixture text", Some("../root")).is_err());
}

// Keep fixture signatures visible here for compile-time contract checks.
const _: fn(&Request) -> (u16, String) = common;
