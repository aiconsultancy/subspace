use super::{client, common, connected, detail, page, row, wait};
use crate::{
    ListQuery, ListTab,
    hub::{Hub, HubSnapshot, HubStatus, IssueStatus, ReadError},
    model::IssueListItem,
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};
use subspace_framework::client_test_support::Mock;

#[test]
fn offline_refresh_keeps_the_loaded_issue_children_and_read_time_until_retry() {
    let failure = Arc::new(AtomicU16::new(0));
    let response = failure.clone();
    let mock = Mock::new(move |request| {
        if request.path == "/api/v1/issues/SUB-1" {
            if response.load(Ordering::SeqCst) != 0 {
                return (0, String::new());
            }
            let mut issue = detail("1");
            issue["children"] = json!({"items":[row("2")],"nextCursor":"children-next"});
            return (200, issue.to_string());
        }
        if request.path.starts_with("/api/v1/issues?") {
            let url = url::Url::parse(&format!("http://127.0.0.1{}", request.path)).unwrap();
            assert!(
                url.query_pairs()
                    .any(|(key, value)| key == "parent" && value == "1")
            );
            return (200, page(vec![row("3")], None));
        }
        common(request)
    });
    let hub = connected(&mock);
    hub.load_more_subissues().unwrap();
    let loaded = wait(&hub, |s| children(s).len() == 2);
    let read_at = loaded.issue().unwrap().updated_at();
    failure.store(1, Ordering::SeqCst);
    hub.reload_issue().unwrap();
    let offline = wait(&hub, |s| {
        s.issue().unwrap().status() == &IssueStatus::Error(ReadError::Network)
    });
    let issue = offline.issue().unwrap();
    assert_eq!(issue.updated_at(), read_at);
    assert_eq!(issue.detail().unwrap().fields.title, "A fixture issue");
    assert_eq!(children(&offline).len(), 2);
    assert_eq!(children(&offline)[1].fields.identifier, "SUB-3");
    failure.store(0, Ordering::SeqCst);
    hub.reload_issue().unwrap();
    let refreshed = wait(&hub, |s| {
        s.issue().unwrap().status() == &IssueStatus::Loaded
    });
    assert_eq!(children(&refreshed).len(), 1);
    hub.stop();
}

fn children(snapshot: &HubSnapshot) -> &[IssueListItem] {
    &snapshot
        .issue()
        .unwrap()
        .detail()
        .unwrap()
        .children
        .as_ref()
        .unwrap()
        .items
}

#[test]
fn every_children_cursor_continues_with_the_detail_uuid() {
    const ID: &str = "00000000-0000-4000-8000-000000000001";
    let mock = Mock::new(|request| {
        if request.path == "/api/v1/issues/SUB-1" {
            let mut issue = detail("1");
            issue["id"] = json!(ID);
            issue["children"] = json!({"items":[row("2")],"nextCursor":"children-1"});
            return (200, issue.to_string());
        }
        if request.path.starts_with("/api/v1/issues?") {
            let url = url::Url::parse(&format!("http://127.0.0.1{}", request.path)).unwrap();
            let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
            assert_eq!(query.get("parent").map(|value| value.as_ref()), Some(ID));
            assert_eq!(
                query.get("order").map(|value| value.as_ref()),
                Some("manual")
            );
            return match query.get("cursor").map(|value| value.as_ref()) {
                Some("children-1") => (200, page(vec![row("3")], Some("children-2"))),
                Some("children-2") => (200, page(vec![row("4")], None)),
                cursor => panic!("unexpected children cursor {cursor:?}"),
            };
        }
        common(request)
    });
    let hub = connected(&mock);
    for count in [2, 3] {
        hub.load_more_subissues().unwrap();
        let snapshot = wait(&hub, |s| children(s).len() == count);
        assert!(!snapshot.issue().unwrap().children_failed());
    }
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.path.contains("parent="))
            .count(),
        2
    );
    hub.stop();
}

#[test]
fn refresh_discards_loaded_content_when_the_issue_is_unavailable_or_access_fails() {
    for status in [401, 403, 404, 500] {
        let failure = Arc::new(AtomicU16::new(0));
        let response = failure.clone();
        let mock = Mock::new(move |request| {
            let code = response.load(Ordering::SeqCst);
            if request.path == "/api/v1/issues/SUB-1" && code != 0 {
                return (code, "{}".into());
            }
            common(request)
        });
        let hub = connected(&mock);
        failure.store(status, Ordering::SeqCst);
        hub.reload_issue().unwrap();
        let failed = wait(&hub, |s| {
            matches!(
                s.issue().unwrap().status(),
                IssueStatus::Error(_) | IssueStatus::Unavailable
            )
        });
        assert!(failed.issue().unwrap().detail().is_none(), "HTTP {status}");
        assert!(
            failed.issue().unwrap().updated_at().is_none(),
            "HTTP {status}"
        );
        hub.stop();
    }
}

#[test]
fn a_rejected_continuation_replaces_the_list_with_the_same_query() {
    let mock = Mock::new(|request| {
        if request.path.contains("cursor=rejected") {
            return (400, "{}".into());
        }
        if request.path.contains("/issues/groups?") {
            return (
                200,
                json!({"groupBy":"state","groups":[
                    {"stateId":"state-1","stateType":"started","count":2,"items":[row("1")],"nextCursor":"rejected"}
                ]})
                .to_string(),
            );
        }
        common(request)
    });
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    wait(&hub, |s| s.status() == &HubStatus::Ready);
    let query = ListQuery::default().project(Some("SUB")).search("fixture");
    hub.activate(query).unwrap();
    let initial = wait(&hub, |s| s.list().is_some());
    hub.load_more(0).unwrap();
    let recovered = wait(&hub, |s| {
        s.list_id() != initial.list_id() && !s.list_loading()
    });
    assert!(!recovered.page_loading(0));
    assert!(!recovered.page_failed(0));
    let requests = mock.requests();
    let groups: Vec<_> = requests
        .iter()
        .filter(|r| r.path.contains("/issues/groups?") && !r.path.contains("limitPerGroup=0"))
        .collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].path, groups[1].path);
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.path.contains("cursor=rejected"))
            .count(),
        1
    );
    hub.stop();
}

#[test]
fn a_bad_first_page_or_server_failure_stays_retryable_without_replacing_the_list() {
    for (cursor, status) in [(None, 400), (Some("next"), 500)] {
        let mock = Mock::new(move |request| {
            if request.path.starts_with("/api/v1/issues?") {
                return (status, "{}".into());
            }
            if request.path.contains("/issues/groups?") {
                return (
                    200,
                    json!({"groupBy":"stateType","groups":[
                        {"stateType":"started","count":2,"items":[],"nextCursor":cursor}
                    ]})
                    .to_string(),
                );
            }
            common(request)
        });
        let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
        wait(&hub, |s| s.status() == &HubStatus::Ready);
        hub.activate(ListQuery::default()).unwrap();
        let initial = wait(&hub, |s| s.list().is_some());
        hub.load_more(0).unwrap();
        let failed = wait(&hub, |s| s.page_failed(0));
        assert_eq!(failed.list_id(), initial.list_id());
        assert!(!failed.page_loading(0));
        hub.stop();
    }
}

#[test]
fn a_wider_search_can_replace_only_the_empty_list_it_checked() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mock = Mock::new(|request| {
        if request.path.starts_with("/api/v1/issues/groups?")
            && request.path.contains("q=relay")
            && !request.path.contains("stateType=")
        {
            return (200, json!({
                "groupBy":"stateType",
                "groups":[{"stateType":"started","count":1,"items":[row("1")],"nextCursor":null}]
            }).to_string());
        }
        common(request)
    });
    let scoped = ListQuery::new(ListTab::Backlog).search("relay");
    let wider = ListQuery::new(ListTab::All).search("relay");
    let hub = Hub::from_client(client(&mock), "Subspace API");
    wait(&hub, |s| s.status() == &HubStatus::Ready);
    hub.activate(scoped.clone()).unwrap();
    let empty =
        |s: &HubSnapshot| !s.list_loading() && s.list().is_some_and(|list| list.groups.is_empty());
    wait(&hub, empty);
    let stale = runtime
        .block_on(hub.find_wider_matches(&scoped, &wider))
        .unwrap();
    hub.activate(ListQuery::new(ListTab::Active).search("another"))
        .unwrap();
    assert!(!hub.adopt_wider_matches(stale));
    hub.activate(scoped.clone()).unwrap();
    wait(&hub, empty);
    let found = runtime
        .block_on(hub.find_wider_matches(&scoped, &wider))
        .unwrap();
    assert!(
        empty(&hub.snapshot()),
        "the user has not widened the search yet"
    );
    assert!(hub.adopt_wider_matches(found));
    assert_eq!(
        hub.snapshot().list().unwrap().groups[0].items[0]
            .fields
            .identifier,
        "SUB-1"
    );
    hub.stop();
}
