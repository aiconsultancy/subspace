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
    mock.config(&json!({"slowBody": {"path": put, "seconds": 2.0, "times": 1}}));
    // Save A is stored, but its answer is slow; the issue is opened again
    // and save B starts on the same item before A's answer lands. B's own
    // answer is slower still, so it is pending whenever A's lands.
    hub.set_todo_done(&item.id, true).unwrap();
    wait_for("A sent", &|| mock.count("PUT", &item.id) == 1);
    hub.close_issue();
    open_todos(&hub);
    wait_for("A stored", &|| {
        hub.snapshot().issue().is_some_and(|issue| {
            issue.todos().unwrap()[0]
                .items
                .iter()
                .any(|row| row.id == item.id && row.completed)
        })
    });
    mock.config(&json!({"slowBody": {"path": put, "seconds": 6.0, "times": 1}}));
    hub.set_todo_done(&item.id, false).unwrap();
    // A's late answer reads the lists again: once it has, B must still be
    // pending, not re-enabled by A's answer. Look for a list read after the
    // mock answered A, since on a busy machine A can land before this
    // point, and a count taken here would miss its read.
    wait_for("A's answer, reconciled", &|| {
        read_after_answer(&mock, &item.id)
    });
    assert!(
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.todo_saving(&item.id)),
        "B is still pending after A's answer"
    );
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

/// Whether the mock logged a read of the lists after it answered the first
/// save of `item`.
fn read_after_answer(mock: &NettMock, item: &str) -> bool {
    let log = mock.requests();
    let answered = log
        .iter()
        .find(|r| r["method"] == "PUT" && r["path"].as_str().is_some_and(|p| p.ends_with(item)))
        .and_then(|r| r["answeredAt"].as_f64());
    answered.is_some_and(|answered| {
        log.iter().any(|r| {
            r["method"] == "GET"
                && r["path"].as_str().is_some_and(|p| p.ends_with("/items"))
                && r["at"].as_f64().is_some_and(|at| at >= answered)
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
