use super::{page, row};
use crate::hub::{DecisionReadStatus, Hub, IssueSnapshot, ReadError};
use crate::tests::{common, connected, wait};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU16, Ordering},
};
use subspace_framework::client_test_support::Mock;

const FIRST: &str = "/api/v1/decisions?issue=SUB-1&limit=50";
const NEXT: &str = "/api/v1/decisions?issue=SUB-1&limit=50&cursor=next";
const NEW: &str = "00000000-0000-0000-0000-000000000001";
const OLD: &str = "00000000-0000-0000-0000-000000000003";

fn issue(hub: &Hub, condition: impl Fn(&IssueSnapshot) -> bool) -> IssueSnapshot {
    wait(hub, |snapshot| snapshot.issue().is_some_and(&condition))
        .issue()
        .unwrap()
        .clone()
}

fn ids(issue: &IssueSnapshot) -> Vec<&str> {
    issue
        .decisions()
        .unwrap()
        .items
        .iter()
        .map(|row| row.id.as_str())
        .collect()
}

#[test]
fn rows_are_lazy_ordered_deduplicated_and_refreshed_to_the_loaded_depth() {
    let refreshed = Arc::new(AtomicBool::new(false));
    let now = refreshed.clone();
    let mock = Mock::new(move |request| match request.path.as_str() {
        FIRST => (200, page(vec![row(), row()], Some("next"))),
        NEXT => {
            let mut older = row();
            older["id"] = json!(OLD);
            older["title"] = json!(if now.load(Ordering::Relaxed) {
                "Fresh older"
            } else {
                "Older"
            });
            (200, page(vec![row(), older], None))
        }
        _ => common(request),
    });
    let hub = connected(&mock);
    assert!(
        !mock
            .requests()
            .iter()
            .any(|r| r.path.contains("/decisions"))
    );
    hub.activate_decisions().unwrap();
    assert_eq!(ids(&issue(&hub, |i| i.decisions().is_some())), [NEW]);
    hub.load_more_decisions().unwrap();
    assert_eq!(
        ids(&issue(&hub, |i| i
            .decisions()
            .is_some_and(|p| p.next_cursor.is_none()))),
        [NEW, OLD]
    );
    hub.activate_decisions().unwrap();
    refreshed.store(true, Ordering::Relaxed);
    hub.reload_issue().unwrap();
    let current = issue(&hub, |i| {
        i.decisions()
            .is_some_and(|p| p.items.iter().any(|row| row.title == "Fresh older"))
    });
    assert_eq!(ids(&current), [NEW, OLD]);
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.path == FIRST || r.path == NEXT)
            .count(),
        4
    );
    hub.stop();
}

#[test]
fn failures_keep_the_cursor_until_retry_and_404_retires_the_rows() {
    let code = Arc::new(AtomicU16::new(0));
    let status = code.clone();
    let mock = Mock::new(move |request| match request.path.as_str() {
        FIRST => (200, page(vec![row()], Some("next"))),
        NEXT => (status.load(Ordering::Relaxed), "{}".into()),
        _ => common(request),
    });
    let hub = connected(&mock);
    hub.activate_decisions().unwrap();
    issue(&hub, |i| i.decisions().is_some());
    hub.load_more_decisions().unwrap();
    let failed = issue(&hub, |i| i.decisions_failed() == Some(&ReadError::Network));
    assert_eq!(ids(&failed), [NEW]);
    assert_eq!(
        failed.decisions().unwrap().next_cursor.as_deref(),
        Some("next")
    );
    code.store(404, Ordering::Relaxed);
    hub.retry_decisions().unwrap();
    let unavailable = issue(&hub, IssueSnapshot::decisions_unavailable);
    assert!(unavailable.decisions().is_none());
    assert!(unavailable.decisions_failed().is_none());
    assert_eq!(mock.requests().iter().filter(|r| r.path == NEXT).count(), 2);
    hub.stop();
}

#[test]
fn detail_distinguishes_gone_from_a_retryable_bad_request() {
    let mock = Mock::new(
        |request| match request.path.split('/').next_back().unwrap() {
            NEW => (404, "{}".into()),
            OLD => (400, "{}".into()),
            _ => common(request),
        },
    );
    let hub = connected(&mock);
    hub.open_decision(NEW).unwrap();
    let gone = issue(&hub, |i| {
        matches!(i.decision(NEW), Some(DecisionReadStatus::Gone))
    });
    assert!(gone.decision(OLD).is_none());
    hub.open_decision(OLD).unwrap();
    let failed = issue(&hub, |i| {
        matches!(i.decision(OLD), Some(DecisionReadStatus::Error(_)))
    });
    assert!(matches!(
        failed.decision(OLD),
        Some(DecisionReadStatus::Error(ReadError::Response {
            status: Some(400)
        }))
    ));
    hub.stop();
}

#[test]
fn a_partial_refresh_keeps_old_pages_and_retry_starts_a_new_cursor_chain() {
    let phase = Arc::new(AtomicU16::new(0));
    let now = phase.clone();
    let mock = Mock::new(move |request| match request.path.as_str() {
        FIRST => match now.load(Ordering::Relaxed) {
            0 => (200, page(vec![named(NEW, "Original head")], Some("next"))),
            1 => (200, page(vec![named(NEW, "Partial head")], Some("broken"))),
            _ => (200, page(vec![named(NEW, "Retry head")], Some("fresh"))),
        },
        NEXT => (200, page(vec![named(OLD, "Original tail")], Some("held"))),
        "/api/v1/decisions?issue=SUB-1&limit=50&cursor=broken" => (500, "{}".into()),
        "/api/v1/decisions?issue=SUB-1&limit=50&cursor=fresh" => {
            (200, page(vec![named(OLD, "Retry tail")], Some("done")))
        }
        _ => common(request),
    });
    let hub = connected(&mock);
    hub.activate_decisions().unwrap();
    issue(&hub, |i| i.decisions().is_some());
    hub.load_more_decisions().unwrap();
    let old = issue(&hub, |i| cursor(i) == Some("held"));
    phase.store(1, Ordering::Relaxed);
    hub.reload_issue().unwrap();
    let failed = issue(&hub, IssueSnapshot::decisions_refresh_failed);
    assert_eq!(
        rows(&failed),
        [(NEW, "Original head"), (OLD, "Original tail")]
    );
    assert_eq!(rows(&failed), rows(&old));
    assert_eq!(cursor(&failed), cursor(&old));
    phase.store(2, Ordering::Relaxed);
    hub.retry_decisions().unwrap();
    let fresh = issue(&hub, |i| cursor(i) == Some("done"));
    assert_eq!(rows(&fresh), [(NEW, "Retry head"), (OLD, "Retry tail")]);
    let requests = mock.requests();
    let paths: Vec<_> = requests
        .iter()
        .map(|r| r.path.as_str())
        .filter(|p| p.starts_with(FIRST))
        .collect();
    assert_eq!(
        paths,
        [
            FIRST,
            NEXT,
            FIRST,
            "/api/v1/decisions?issue=SUB-1&limit=50&cursor=broken",
            FIRST,
            "/api/v1/decisions?issue=SUB-1&limit=50&cursor=fresh"
        ]
    );
    hub.stop();
}

fn named(id: &str, title: &str) -> serde_json::Value {
    let mut item = row();
    item["id"] = json!(id);
    item["title"] = json!(title);
    item
}

fn rows(issue: &IssueSnapshot) -> Vec<(&str, &str)> {
    issue
        .decisions()
        .unwrap()
        .items
        .iter()
        .map(|item| (item.id.as_str(), item.title.as_str()))
        .collect()
}

fn cursor(issue: &IssueSnapshot) -> Option<&str> {
    issue
        .decisions()
        .and_then(|page| page.next_cursor.as_deref())
}
