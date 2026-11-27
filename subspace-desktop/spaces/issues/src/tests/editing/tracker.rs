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
        ("GET", "/api/v1/issues") if r.path.contains("parentFor=1") => {
            let mut parent = super::row("3", "Roadmap");
            parent["type"] = json!("tracker");
            Some((200, json!({"items":[parent],"nextCursor":null}).to_string()))
        }
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

#[gpui_kit::test]
fn tracker_parent_and_child_changes_wait_for_both_confirmations(cx: &mut TestAppContext) {
    let (fixture, board, cx) = open_tracker_editor(cx);
    cx.update(|w, cx| w.press("e", cx));
    render(cx);
    for (field, option) in [
        ("issue-edit-state", "Todo"),
        ("issue-edit-assignee", "Orbit"),
    ] {
        super::properties::tab_to_visible_field(cx, field);
        cx.update(|w, cx| w.press("space", cx));
        super::properties::wait_for_option(cx, 1);
        cx.update(|w, cx| {
            w.input(option, cx);
            w.press("enter", cx);
        });
        render(cx);
    }
    super::properties::tab_to_visible_field(cx, "issue-parent-picker");
    cx.update(|w, cx| w.press("space", cx));
    super::properties::wait_for_option(cx, 1);
    cx.update(|w, cx| {
        w.press("down", cx);
        w.press("enter", cx);
    });
    render(cx);
    assert!(crate::tests::labelled(cx, "Parent: SUB-3"));

    let answer = |cx: &mut VisualTestContext, id: &'static str| {
        cx.update(|w, cx| w.click(id, cx));
        render(cx);
    };
    answer(cx, "issue-editor-save");
    assert!(crate::tests::labelled(
        cx,
        "Assign the unassigned child issues as well?"
    ));
    assert!(cx.update(|w, _| w.find("issue-editor-notice").visible()));
    answer(cx, "issue-tracker-children-yes");
    assert!(crate::tests::labelled(
        cx,
        "Move the open child issues to this state?"
    ));
    answer(cx, "issue-tracker-children-no");
    assert!(patch_bodies(&fixture).is_empty());

    answer(cx, "issue-editor-save");
    answer(cx, "issue-tracker-children-yes");
    assert!(patch_bodies(&fixture).is_empty());
    answer(cx, "issue-tracker-children-yes");
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({
            "version":1, "parentId":"SUB-3", "assigneeId":"agent-fixture",
            "assignUnassignedChildren":true, "stateId":"state-todo", "moveOpenChildren":true
        })]
    );
}
