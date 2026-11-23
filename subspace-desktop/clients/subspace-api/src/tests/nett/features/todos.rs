use super::super::{server::NettMock, wait_for};
use super::{org_path, own_client};
use crate::hub::Hub;
use crate::issue_writes::IssueSaveError;
use crate::nett::TodoItem;
use serde_json::json;

#[test]
fn a_busy_to_do_save_is_reported_as_busy() {
    let mock = NettMock::start();
    let client = own_client(&mock, "nett-fixture-todo-busy");
    let lists = client.issue_lists("OPS-1").unwrap();
    let items = client.issue_list_items("OPS-1", &lists[0].id).unwrap();
    let issue = client.issue("OPS-1").unwrap().fields.id;
    let path = org_path(
        &mock,
        &format!("issues/{issue}/lists/{}/items/{}", lists[0].id, items[1].id),
    );
    mock.fail_next("PUT", &path, 503, Some(0), 1);
    assert_eq!(
        client
            .set_issue_list_item_done("OPS-1", &items[1], true)
            .err(),
        Some(IssueSaveError::RateLimited)
    );
}

#[test]
fn a_late_to_do_answer_leaves_a_newer_save_pending_and_is_read_again() {
    let mock = NettMock::start();
    let hub = Hub::from_client(own_client(&mock, "nett-fixture-todo-late"), "NETT");
    open_todos(&hub);
    let (put, item) = todo_save_target(&mock, &hub);
    // Both saves' answers are held until the test lets each go: A is
    // stored but answered only once B is on its way, and B stays unanswered
    // while A's answer is handled. The fixture's limit on a hold is longer
    // than the waits before each release, so only a release lets one go.
    mock.config(
        &json!({"slowBody": {"path": put, "seconds": 120.0, "times": 2, "waitForRelease": true}}),
    );
    hub.set_todo_done(&item.id, true).unwrap();
    wait_for("A sent", &|| mock.count("PUT", &item.id) == 1);
    hub.close_issue();
    open_todos(&hub);
    wait_for_a_stored_and_held(&mock, &hub, &item.id);
    hub.set_todo_done(&item.id, false).unwrap();
    wait_for("B held", &|| {
        saves(&mock, &item.id)
            .get(1)
            .is_some_and(|b| b["slowBodyHeadersStatus"] == 200)
    });
    mock.post_hook("slowBody/release", &json!({}));
    // A's late answer reads the lists again: once it has, B must still be
    // pending, not re-enabled by A's answer.
    wait_for("A's answer, reconciled", &|| {
        read_after_answer(&mock, &item.id)
    });
    let saves = saves(&mock, &item.id);
    assert!(
        saves[1]["seq"].as_u64() < saves[0]["answerSeq"].as_u64(),
        "B was sent before A was answered"
    );
    assert!(saves[1]["answerSeq"].is_null(), "B is still unanswered");
    assert!(
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.todo_saving(&item.id)),
        "B is still pending after A's answer"
    );
    mock.post_hook("slowBody/release", &json!({}));
    wait_for("B saved", &|| {
        hub.snapshot().issue().is_some_and(|issue| {
            !issue.todo_saving(&item.id)
                && issue.todos().unwrap()[0]
                    .items
                    .iter()
                    .any(|row| row.id == item.id && !row.completed)
        })
    });
    hub.stop();
}

/// Waits until save A of `item` is stored and its answer is held. A is
/// stored before its answer is held: B must not start until A's hold is
/// first in the release queue.
fn wait_for_a_stored_and_held(mock: &NettMock, hub: &Hub, item: &str) {
    wait_for("A stored", &|| {
        hub.snapshot().issue().is_some_and(|issue| {
            issue.todos().unwrap()[0]
                .items
                .iter()
                .any(|row| row.id == item && row.completed)
        })
    });
    wait_for("A held", &|| {
        saves(mock, item)
            .first()
            .is_some_and(|a| a["slowBodyHeadersStatus"] == 200)
    });
}

/// The logged saves of `item`, oldest first.
fn saves(mock: &NettMock, item: &str) -> Vec<serde_json::Value> {
    mock.requests()
        .into_iter()
        .filter(|r| r["method"] == "PUT" && r["path"].as_str().is_some_and(|p| p.ends_with(item)))
        .collect()
}

/// Whether the mock logged a read of the lists after the first save of
/// `item` was answered. The mock numbers an answer just before its last
/// bytes go, so a read the answer caused always comes after it.
fn read_after_answer(mock: &NettMock, item: &str) -> bool {
    let log = mock.requests();
    let answered = saves(mock, item)
        .first()
        .and_then(|a| a["answerSeq"].as_u64());
    answered.is_some_and(|answered| {
        log.iter().any(|r| {
            r["method"] == "GET"
                && r["path"].as_str().is_some_and(|p| p.ends_with("/items"))
                && r["seq"].as_u64().is_some_and(|seq| seq > answered)
        })
    })
}

fn open_todos(hub: &Hub) {
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait_for("the lists", &|| {
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.todos().is_some())
    });
}

fn todo_save_target(mock: &NettMock, hub: &Hub) -> (String, TodoItem) {
    let snapshot = hub.snapshot();
    let issue = snapshot.issue().unwrap();
    let issue_id = issue.detail().unwrap().fields.id.clone();
    let item = issue.todos().unwrap()[0].items[1].clone();
    let put = org_path(
        mock,
        &format!("issues/{issue_id}/lists/{}/items/{}", item.list_id, item.id),
    );
    (put, item)
}
