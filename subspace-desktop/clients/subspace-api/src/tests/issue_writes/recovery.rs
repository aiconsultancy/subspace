use super::changes;
use crate::tests::{client, common, connected, detail, page, row, wait};
use crate::{
    hub::{Hub, HubStatus},
    issue_writes::{CreateKey, IssueSave, IssueSaveError},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use subspace_framework::client_test_support::{Mock, Request};

fn create_hub(mock: &Mock) -> Hub {
    let hub = Hub::from_client(client(mock), "Fixture");
    wait(&hub, |s| s.status() == &HubStatus::Ready);
    hub
}

#[test]
fn ambiguous_create_requires_a_complete_check_then_an_explicit_second_post() {
    let posts = Arc::new(AtomicUsize::new(0));
    let attempts = posts.clone();
    let mock = Mock::new(move |r| {
        if r.method == "POST" {
            attempts.fetch_add(1, Ordering::SeqCst);
            return (500, "{}".into());
        }
        if r.path.starts_with("/api/v1/issues?") {
            assert!(
                r.path.contains("project=SUB")
                    && r.path.contains("creator=me")
                    && r.path.contains("q=A+fixture+issue")
            );
            return (200, page(vec![row("old")], None));
        }
        common(r)
    });
    let hub = create_hub(&mock);
    let scope = hub.issue_write_scope().unwrap();
    let key = CreateKey::new();
    assert_eq!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .unwrap_err(),
        IssueSaveError::Unconfirmed
    );
    assert!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .is_err()
    );
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    assert!(hub.check_created_issue(&scope, &key).unwrap().is_none());
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    assert!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .is_err()
    );
    assert_eq!(posts.load(Ordering::SeqCst), 2);
}

#[test]
fn recovery_rejects_old_identical_titles_and_reads_every_page_without_clocks() {
    let posts = Arc::new(AtomicUsize::new(0));
    let attempts = posts.clone();
    let mock = Mock::new(move |r| recovery_route(r, &attempts));
    let hub = create_hub(&mock);
    let scope = hub.issue_write_scope().unwrap();
    let key = CreateKey::new();
    assert!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .is_err()
    );
    let found = hub.check_created_issue(&scope, &key).unwrap().unwrap();
    assert_eq!(found.fields.id, "new");
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    assert!(matches!(
        hub.create_issue(&scope, "project-1", &key, &changes()),
        Ok(IssueSave::Saved(_))
    ));
    assert_eq!(posts.load(Ordering::SeqCst), 1);
}

fn recovery_route(r: &Request, posts: &AtomicUsize) -> (u16, String) {
    if r.method == "POST" {
        posts.fetch_add(1, Ordering::SeqCst);
        return (0, String::new());
    }
    if r.path.starts_with("/api/v1/issues?") {
        if !r.path.contains("cursor=second") {
            return (200, page(vec![row("old")], Some("second")));
        }
        return (
            200,
            page(
                if posts.load(Ordering::SeqCst) > 0 {
                    vec![row("new")]
                } else {
                    vec![]
                },
                None,
            ),
        );
    }
    if r.path == "/api/v1/issues/new" {
        let mut issue = detail("new");
        issue["createdAt"] = json!("2000-01-01T00:00:00Z");
        return (200, issue.to_string());
    }
    common(r)
}

#[test]
fn failed_preflight_sends_nothing_and_failed_checks_do_not_enable_create_again() {
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let mock = Mock::new(move |r| {
        if r.path.starts_with("/api/v1/issues?") {
            return match count.fetch_add(1, Ordering::SeqCst) {
                1 => (200, page(vec![], None)),
                _ => (500, "{}".into()),
            };
        }
        if r.method == "POST" {
            return (500, "{}".into());
        }
        common(r)
    });
    let hub = create_hub(&mock);
    let scope = hub.issue_write_scope().unwrap();
    let key = CreateKey::new();
    assert_eq!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .unwrap_err(),
        IssueSaveError::NotSent
    );
    assert!(!mock.requests().iter().any(|r| r.method == "POST"));
    assert_eq!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .unwrap_err(),
        IssueSaveError::Unconfirmed
    );
    assert!(hub.check_created_issue(&scope, &key).is_err());
    assert!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .is_err()
    );
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
}

#[test]
fn cyclic_preflight_and_stopped_scopes_cannot_send() {
    let mock = Mock::new(|r| {
        if r.path.starts_with("/api/v1/issues?") {
            (200, page(vec![], Some("cycle")))
        } else {
            common(r)
        }
    });
    let hub = create_hub(&mock);
    let scope = hub.issue_write_scope().unwrap();
    assert!(
        hub.create_issue(&scope, "project-1", &CreateKey::new(), &changes())
            .is_err()
    );
    hub.stop();
    assert!(
        hub.create_issue(&scope, "project-1", &CreateKey::new(), &changes())
            .is_err()
    );
    assert!(!mock.requests().iter().any(|r| r.method == "POST"));
}

#[test]
fn latest_issue_and_stale_versions_never_blindly_overwrite() {
    let mock = Mock::new(|r| {
        if r.path == "/api/v1/issues/1" {
            let mut issue = detail("1");
            issue["version"] = json!(2);
            return (200, issue.to_string());
        }
        common(r)
    });
    let hub = connected(&mock);
    let scope = hub.issue_write_scope().unwrap();
    assert_eq!(hub.latest_issue(&scope, "1").unwrap().fields.version, 2);
    assert_eq!(
        hub.update_issue(&scope, "1", 1, &changes()).unwrap_err(),
        IssueSaveError::Conflict
    );
    assert!(!mock.requests().iter().any(|r| r.method == "PATCH"));
}

#[test]
fn several_new_matches_keep_create_frozen_without_another_post() {
    let posts = Arc::new(AtomicUsize::new(0));
    let count = posts.clone();
    let mock = Mock::new(move |r| {
        if r.method == "POST" {
            count.fetch_add(1, Ordering::SeqCst);
            return (500, "{}".into());
        }
        if r.path.starts_with("/api/v1/issues?") {
            let rows = if count.load(Ordering::SeqCst) == 0 {
                vec![]
            } else {
                vec![row("new-1"), row("new-2")]
            };
            return (200, page(rows, None));
        }
        common(r)
    });
    let hub = create_hub(&mock);
    let scope = hub.issue_write_scope().unwrap();
    let key = CreateKey::new();
    assert_eq!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .unwrap_err(),
        IssueSaveError::Unconfirmed
    );
    assert_eq!(
        hub.check_created_issue(&scope, &key).unwrap_err(),
        IssueSaveError::SeveralMatches
    );
    assert_eq!(
        hub.create_issue(&scope, "project-1", &key, &changes())
            .unwrap_err(),
        IssueSaveError::Unconfirmed
    );
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    assert!(
        IssueSaveError::SeveralMatches
            .to_string()
            .contains("Check the issue list")
    );
    hub.stop();
}

#[test]
fn patch_conflicts_read_archive_state_and_do_not_offer_version_recovery() {
    let patches = Arc::new(AtomicUsize::new(0));
    let count = patches.clone();
    let mock = Mock::new(move |r| {
        if r.method == "PATCH" {
            count.fetch_add(1, Ordering::SeqCst);
            return (409, "{}".into());
        }
        if r.path == "/api/v1/issues/1" {
            let mut issue = detail("1");
            if count.load(Ordering::SeqCst) > 0 {
                issue["archivedAt"] = json!("2026-11-16T12:00:00Z");
            }
            return (200, issue.to_string());
        }
        common(r)
    });
    let hub = connected(&mock);
    let scope = hub.issue_write_scope().unwrap();
    assert_eq!(
        hub.update_issue(&scope, "1", 1, &changes()).unwrap_err(),
        IssueSaveError::IssueArchived
    );
    assert_eq!(
        hub.update_issue(&scope, "1", 1, &changes()).unwrap_err(),
        IssueSaveError::IssueArchived
    );
    assert_eq!(patches.load(Ordering::SeqCst), 1);
    assert!(
        IssueSaveError::IssueArchived
            .to_string()
            .contains("Restore it before saving")
    );
    hub.stop();
}
