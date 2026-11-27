use super::{board_with_hub, has, offline_with, patch_bodies, render, settle_board};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

fn open_tracker_editor(
    cx: &mut TestAppContext,
) -> (
    super::Offline,
    Entity<crate::Issues>,
    &mut VisualTestContext,
) {
    let fixture = offline_with(|r| {
        match (r.method.as_str(), r.path.split('?').next().unwrap()) {
        ("PATCH" | "GET", "/api/v1/issues/1") => {
            let mut detail = super::row("1", "Tracker");
            detail["todos"] = json!([]);
            detail["type"] = json!("tracker");
            detail["state"] = json!({"id":"state-backlog","name":"Backlog","type":"backlog"});
            detail["version"] = json!(if r.method == "PATCH" { 2 } else { 1 });
            Some((200, detail.to_string()))
        }
        ("GET", "/api/v1/users") => Some((200, json!({"items":[{"id":"agent-fixture","displayName":"Orbit Fixture","kind":"agent"}],"nextCursor":null}).to_string())),
        ("GET", path) if path.ends_with("/workflow-states") => Some((200, json!({"items":[
            {"id":"state-backlog","projectId":"project-SUB","name":"Backlog","type":"backlog","position":0,"isDefault":true},
            {"id":"state-todo","projectId":"project-SUB","name":"Todo","type":"unstarted","position":1,"isDefault":false},
            {"id":"state-started","projectId":"project-SUB","name":"In Progress","type":"started","position":2,"isDefault":false}
        ],"nextCursor":null}).to_string())),
        _ => None,
    }
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    board.update(cx, |b, cx| b.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |b| b.can_edit_issue());
    render(cx);
    (fixture, board, cx)
}

fn choose_tracker_edit(cx: &mut VisualTestContext, property: &str, option: &str) {
    cx.update(|w, cx| {
        w.click(
            gpui_kit::SharedString::from(format!("issue-property-edit:{property}")),
            cx,
        )
    });
    super::properties::wait_for_option(cx, 1);
    cx.update(|w, cx| {
        w.input(option, cx);
        w.press("enter", cx);
    });
    render(cx);
    cx.update(|w, cx| w.click("issue-inline-save", cx));
    render(cx);
    assert!(has(cx, "issue-tracker-children-yes"));
}

fn tracker_assignment(cx: &mut TestAppContext, answer: &str, expected: serde_json::Value) {
    let (fixture, board, cx) = open_tracker_editor(cx);
    choose_tracker_edit(cx, "Assignee", "Orbit");
    assert!(patch_bodies(&fixture).is_empty());
    cx.update(|w, cx| {
        w.click(
            gpui_kit::SharedString::from(format!("issue-tracker-children-{answer}")),
            cx,
        )
    });
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(patch_bodies(&fixture), [expected]);
}

#[gpui_kit::test]
fn tracker_assignment_no_saves_only_the_tracker(cx: &mut TestAppContext) {
    tracker_assignment(cx, "no", json!({"version":1,"assigneeId":"agent-fixture"}));
}

#[gpui_kit::test]
fn tracker_assignment_yes_requests_unassigned_children(cx: &mut TestAppContext) {
    tracker_assignment(
        cx,
        "yes",
        json!({"version":1,"assigneeId":"agent-fixture","assignUnassignedChildren":true}),
    );
}

#[gpui_kit::test]
fn tracker_state_cancellation_keeps_the_draft_and_confirmation_moves_children(
    cx: &mut TestAppContext,
) {
    let (fixture, board, cx) = open_tracker_editor(cx);
    choose_tracker_edit(cx, "State", "Todo");
    cx.update(|w, cx| w.click("issue-tracker-children-no", cx));
    render(cx);
    assert!(patch_bodies(&fixture).is_empty());
    assert!(has(cx, "issue-inline-save"));
    cx.update(|w, cx| w.click("issue-inline-save", cx));
    render(cx);
    cx.update(|w, cx| w.click("issue-tracker-children-yes", cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"stateId":"state-todo","moveOpenChildren":true})]
    );
}

#[gpui_kit::test]
fn reselecting_the_trackers_derived_state_can_move_its_children(cx: &mut TestAppContext) {
    let (fixture, board, cx) = open_tracker_editor(cx);
    choose_tracker_edit(cx, "State", "Backlog");
    cx.update(|w, cx| w.click("issue-tracker-children-yes", cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"stateId":"state-backlog","moveOpenChildren":true})]
    );
}
