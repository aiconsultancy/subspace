use super::{client, common, page, row, wait};
use crate::{
    ActivityQuery,
    hub::{ActivitySection, Hub, HubStatus},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use subspace_framework::client_test_support::Mock;

#[test]
fn recent_and_trackers_use_activity_order_and_exact_filters() {
    let mock = Mock::new(|_| (200, page(vec![], None)));
    let api = client(&mock);
    let query = ActivityQuery::at(
        chrono::DateTime::parse_from_rfc3339("2026-11-16T12:00:00Z")
            .unwrap()
            .into(),
    )
    .project(Some("SUB"));
    assert!(api.activity_issues(&query, None).is_ok());
    assert!(api.trackers_in_progress(&query, Some("later")).is_ok());
    let requests = mock.requests();
    for request in &requests {
        assert!(request.path.contains("order=activity") && request.path.contains("project=SUB"));
    }
    assert!(requests[0].path.contains("activitySince="));
    assert!(!requests[0].path.contains("cursor="));
    assert!(
        requests[1].path.contains("type=tracker")
            && requests[1].path.contains("stateType=started")
            && requests[1].path.contains("cursor=later")
    );
}

#[test]
fn head_refresh_restarts_recent_and_merges_boundary_without_duplicates() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mock = head_fixture(count);
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    let query = ActivityQuery::default();
    hub.activate_activity(query.clone()).unwrap();
    wait(&hub, |snapshot| {
        snapshot
            .activity()
            .is_some_and(|activity| activity.recent().page().is_some())
    });
    hub.load_more_activity(ActivitySection::Recent).unwrap();
    wait(&hub, |snapshot| {
        snapshot
            .activity()
            .unwrap()
            .recent()
            .page()
            .unwrap()
            .items
            .len()
            == 2
    });
    hub.reload_activity(query).unwrap();
    let snapshot = wait(&hub, |snapshot| {
        snapshot
            .activity()
            .unwrap()
            .recent()
            .page()
            .unwrap()
            .items
            .len()
            == 3
    });
    let page = snapshot.activity().unwrap().recent().page().unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|row| row.fields.id.as_str())
            .collect::<Vec<_>>(),
        ["3", "1", "2"]
    );
    assert!(page.next_cursor.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let recent: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|request| request.path.contains("activitySince="))
        .collect();
    assert!(!recent[2].path.contains("cursor="));
    hub.stop();
}

fn head_fixture(count: Arc<AtomicUsize>) -> Mock {
    Mock::new(move |request| {
        if request.path.starts_with("/api/v1/issues?") {
            if request.path.contains("type=tracker") {
                return (200, page(vec![], None));
            }
            let call = count.fetch_add(1, Ordering::SeqCst);
            let mut first = row("1");
            first["lastActivityAt"] = json!("2026-11-15T12:00:00Z");
            let mut second = row("2");
            second["lastActivityAt"] = json!("2026-11-14T12:00:00Z");
            return match call {
                0 => (200, page(vec![first], Some("tail"))),
                1 => (200, page(vec![second], None)),
                _ => (200, page(vec![row("3"), first], Some("new-head"))),
            };
        }
        common(request)
    })
}
