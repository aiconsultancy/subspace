use super::{comment, common, connected, page, wait};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use subspace_framework::client_test_support::{Mock, Request};

fn entry(number: usize, comment: Option<Value>) -> Value {
    json!({"id":format!("activity-{number}"),"kind":if comment.is_some() {"comment.created"} else {"issue.changed"},
        "occurredAt":"2026-11-16T12:00:00Z","displayAt":"2026-11-16T12:00:00Z",
        "actor":{"type":"system","label":"Fixture service","userId":null,"person":null},"comment":comment})
}

fn two_rows(hub: &crate::hub::Hub) -> crate::hub::HubSnapshot {
    wait(hub, |snapshot| {
        snapshot
            .issue()
            .unwrap()
            .timeline()
            .page()
            .is_some_and(|page| page.items.len() == 2)
    })
}

#[test]
fn decision_activity_decodes_optional_and_unknown_payloads() {
    use crate::model::{ActivityPage, IssueActivity};
    let id = "00000000-0000-0000-0000-000000000001";
    let mut value = entry(1, None);
    value["kind"] = json!("decision.logged");
    value["data"] = json!({"decisionId":id,"futureField":{"value":true}});
    let activity: IssueActivity = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(activity.decision_id(), Some(id));
    let page: ActivityPage = serde_json::from_value(json!({"items":[value.clone()],"nextCursor":null,
        "references":{"decisions":[{"id":id,"title":"Fixture decision","status":"futureStatus"}],"future":[]}})).unwrap();
    assert_eq!(
        page.references.decisions[0].status,
        crate::model::DecisionStatus::Other
    );
    let page: ActivityPage = serde_json::from_value(json!({"items":[],"nextCursor":null})).unwrap();
    assert!(page.references.decisions.is_empty());
    let page: ActivityPage = serde_json::from_value(json!({"items":[],"references":null})).unwrap();
    assert!(page.references.decisions.is_empty());
    let page: ActivityPage =
        serde_json::from_value(json!({"items":[],"references":{"decisions":null}})).unwrap();
    assert!(page.references.decisions.is_empty());
    for data in [
        Value::Null,
        json!({}),
        json!({"decisionId":17}),
        json!({"decisionId":"invalid"}),
    ] {
        value["data"] = data;
        let activity: IssueActivity = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(activity.decision_id(), None);
    }
    value.as_object_mut().unwrap().remove("data");
    let activity: IssueActivity = serde_json::from_value(value).unwrap();
    assert_eq!(activity.decision_id(), None);
}

#[test]
fn decision_references_merge_across_timeline_pages() {
    let mock = Mock::new(|request| {
        if request.path.contains("/activity?") {
            let later = request.path.contains("cursor=");
            let id = if later {
                "00000000-0000-0000-0000-000000000002"
            } else {
                "00000000-0000-0000-0000-000000000001"
            };
            let mut item = entry(usize::from(later), None);
            item["kind"] = json!("decision.logged");
            item["data"] = json!({"decisionId":id});
            return (200, json!({"items":[item],"nextCursor":(!later).then_some("older"),
                "references":{"decisions":[{"id":id,"title":"Fixture decision","status":"accepted"}]}}).to_string());
        }
        common(request)
    });
    let hub = connected(&mock);
    hub.activate_timeline(1).unwrap();
    let snapshot = two_rows(&hub);
    let timeline = snapshot.issue().unwrap().timeline();
    assert!(
        timeline
            .decision("00000000-0000-0000-0000-000000000001")
            .is_some()
    );
    assert!(
        timeline
            .decision("00000000-0000-0000-0000-000000000002")
            .is_some()
    );
    assert!(timeline.decision("missing").is_none());
    hub.stop();
}

#[test]
fn timeline_orders_fractional_seconds_as_instants_across_pages() {
    let mock = Mock::new(|request| {
        if request.path.contains("/activity?") {
            let later_page = request.path.contains("cursor=");
            let mut item = entry(usize::from(later_page), None);
            item["displayAt"] = json!(if later_page {
                "2026-11-16T12:00:00.5Z"
            } else {
                "2026-11-16T12:00:00Z"
            });
            return (200, page(vec![item], (!later_page).then_some("fractional")));
        }
        common(request)
    });
    let hub = connected(&mock);
    hub.activate_timeline(1).unwrap();
    let snapshot = two_rows(&hub);
    let items = &snapshot.issue().unwrap().timeline().page().unwrap().items;
    assert_eq!(items[0].id, "activity-1");
    assert_eq!(items[1].id, "activity-0");
    hub.stop();
}

#[test]
fn timeline_automatically_pages_to_comment_goal_with_finite_budget() {
    let (mock, reads) = paged_timeline(false);
    let hub = connected(&mock);
    hub.activate_timeline(10).unwrap();
    let snapshot = wait(&hub, |snapshot| {
        snapshot.issue().unwrap().timeline().page().is_some()
            && !snapshot.issue().unwrap().timeline().loading()
    });
    assert_eq!(reads.load(Ordering::SeqCst), 4);
    assert_eq!(
        snapshot
            .issue()
            .unwrap()
            .timeline()
            .page()
            .unwrap()
            .items
            .len(),
        4
    );
    hub.stop();
}

#[test]
fn timeline_stops_when_comment_goal_is_met_and_pages_only_on_request() {
    let (mock, reads) = paged_timeline(true);
    let hub = connected(&mock);
    hub.activate_timeline(1).unwrap();
    wait(&hub, |snapshot| {
        snapshot.issue().unwrap().timeline().page().is_some()
            && !snapshot.issue().unwrap().timeline().loading()
    });
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    hub.load_more_timeline(2).unwrap();
    wait(&hub, |snapshot| {
        snapshot
            .issue()
            .unwrap()
            .timeline()
            .page()
            .unwrap()
            .items
            .len()
            == 2
    });
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    hub.stop();
}

#[test]
fn post_refresh_retires_projected_comment_without_losing_loaded_history() {
    let posted = Arc::new(std::sync::Mutex::new(None));
    let storage = posted.clone();
    let mock = Mock::new(move |request: &Request| {
        if request.method == "POST" {
            let saved = comment("new-comment", "Fixture text", None);
            *storage.lock().unwrap() = Some(saved.clone());
            return (201, saved.to_string());
        }
        if request.path.contains("/activity?") {
            return (
                200,
                page(vec![entry(1, storage.lock().unwrap().clone())], None),
            );
        }
        common(request)
    });
    let hub = connected(&mock);
    hub.edit_comment("1", None, "Fixture text").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    let snapshot = wait(&hub, |snapshot| {
        snapshot.issue().unwrap().timeline().page().is_some()
    });
    assert!(snapshot.issue().unwrap().timeline().projected().is_empty());
    assert_eq!(
        snapshot.issue().unwrap().timeline().page().unwrap().items[0]
            .comment
            .as_ref()
            .unwrap()
            .id,
        "new-comment"
    );
    hub.refresh_discussion("1").unwrap();
    hub.stop();
}

fn paged_timeline(with_comments: bool) -> (Mock, Arc<AtomicUsize>) {
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let mock = Mock::new(move |request| {
        if request.path.contains("/activity?") {
            let number = count.fetch_add(1, Ordering::SeqCst);
            let comment =
                with_comments.then(|| comment(&format!("comment-{number}"), "Fixture text", None));
            return (
                200,
                page(
                    vec![entry(number, comment)],
                    Some(&format!("cursor-{number}")),
                ),
            );
        }
        common(request)
    });
    (mock, reads)
}
