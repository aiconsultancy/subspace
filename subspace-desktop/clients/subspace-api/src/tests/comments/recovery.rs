use super::post_count;
use crate::{
    hub::CommentState,
    tests::{comment, common, connected, page, wait},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use subspace_framework::client_test_support::Mock;

fn skewed_thread(seconds: i64, parent: Option<&'static str>) -> (Mock, Arc<AtomicUsize>) {
    let posts = Arc::new(AtomicUsize::new(0));
    let counter = posts.clone();
    let mock = Mock::new(move |request| {
        if request.method == "POST" {
            counter.fetch_add(1, Ordering::SeqCst);
            return (0, String::new());
        }
        if request.path.contains("/comments") {
            if !request.path.contains("cursor=") {
                return (200, page(vec![], Some("tail")));
            }
            let mut old = comment("old-identical", "Fixture text", parent);
            old["createdAt"] = json!((chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339());
            let mut items = vec![old];
            if counter.load(Ordering::SeqCst) > 0 {
                let mut saved = comment("saved", "Fixture text", parent);
                saved["createdAt"] =
                    json!((chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339());
                items.push(saved);
            }
            return (200, page(items, None));
        }
        common(request)
    });
    (mock, posts)
}

fn confirm_skewed_post(seconds: i64, parent: Option<&'static str>) -> usize {
    let (mock, posts) = skewed_thread(seconds, parent);
    let hub = connected(&mock);
    if parent.is_some() {
        hub.activate_timeline(0).unwrap();
        wait(&hub, |s| s.issue().unwrap().timeline().page().is_some());
    }
    hub.edit_comment("1", parent, "Fixture text").unwrap();
    hub.submit_comment("1", parent, false).unwrap();
    wait(&hub, |_| {
        matches!(
            hub.comment_draft("1", parent).unwrap().state(),
            CommentState::Unclear { .. }
        )
    });
    hub.check_discussion("1", parent).unwrap();
    let snapshot = wait(&hub, |_| {
        hub.comment_draft("1", parent).unwrap().confirmation() == 1
    });
    assert!(hub.comment_draft("1", parent).unwrap().body().is_empty());
    assert_eq!(
        snapshot.issue().unwrap().timeline().projected()[0].id,
        "saved"
    );
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    hub.stop();
    posts.load(Ordering::SeqCst)
}

#[test]
fn recovery_finds_new_root_and_reply_when_server_clock_is_behind() {
    assert_eq!(confirm_skewed_post(-600, None), 1);
    assert_eq!(confirm_skewed_post(-600, Some("root-1")), 1);
}

#[test]
fn recovery_finds_new_root_and_reply_when_server_clock_is_ahead() {
    assert_eq!(confirm_skewed_post(600, None), 1);
    assert_eq!(confirm_skewed_post(600, Some("root-1")), 1);
}

#[test]
fn failed_or_repeating_preflight_keeps_draft_and_never_posts() {
    for status in [200, 503] {
        let mock = Mock::new(move |request| {
            if request.path.contains("/comments") {
                return (status, page(vec![], Some("repeated")));
            }
            common(request)
        });
        let hub = connected(&mock);
        hub.edit_comment("1", None, "Fixture text").unwrap();
        hub.submit_comment("1", None, false).unwrap();
        wait(&hub, |_| {
            matches!(
                hub.comment_draft("1", None).unwrap().state(),
                CommentState::Failed(crate::WriteError::NotSent)
            )
        });
        assert_eq!(hub.comment_draft("1", None).unwrap().body(), "Fixture text");
        assert!(
            mock.requests()
                .iter()
                .all(|request| request.method == "GET")
        );
        hub.stop();
    }
}

#[test]
fn failed_discussion_read_keeps_draft_frozen_and_blocks_resubmit() {
    let posted = Arc::new(AtomicUsize::new(0));
    let storage = posted.clone();
    let mock = Mock::new(move |request| {
        if request.method == "POST" {
            storage.fetch_add(1, Ordering::SeqCst);
            (503, "opaque error".into())
        } else if request.path.contains("cursor=unavailable") {
            (503, "opaque error".into())
        } else if request.path.starts_with("/api/v1/issues/1/comments?") {
            (
                200,
                page(
                    vec![],
                    (storage.load(Ordering::SeqCst) > 0).then_some("unavailable"),
                ),
            )
        } else {
            common(request)
        }
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
    hub.check_comment("1", None).unwrap();
    wait(&hub, |_| {
        hub.comment_draft("1", None).unwrap().refresh_failed()
    });
    hub.submit_comment("1", None, true).unwrap();
    assert_eq!(post_count(&mock), 1);
    assert!(!hub.comment_draft("1", None).unwrap().is_retry_ready());
    hub.discard_comment("1", None);
    assert!(!hub.has_comment_drafts());
    hub.stop();
}
