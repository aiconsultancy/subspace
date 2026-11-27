use super::{has, label, opened, render};
use crate::tests::{
    labelled,
    offline::{Offline, api, offline_with, row, settle_board},
};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn empty_tracker() -> Offline {
    let saved = AtomicBool::new(false);
    offline_with(move |r| {
        let mut child = row("4", "A new sub-issue");
        child["parentId"] = json!("1");
        if r.method == "POST" && r.path == "/api/v1/issues" {
            child["todos"] = json!([]);
            saved.store(true, Ordering::SeqCst);
            return Some((201, child.to_string()));
        }
        if r.path == "/api/v1/issues/4" {
            child["todos"] = json!([]);
            return Some((200, child.to_string()));
        }
        if r.path == "/api/v1/issues/SUB-1" {
            let mut tracker = api(&r.path, &Default::default());
            tracker["children"] = if saved.load(Ordering::SeqCst) {
                // The first page grew while the draft was open. The new
                // child belongs to a later page and should still be shown.
                json!({"items":[row("3", "Older sub-issue")],"nextCursor":"older-first"})
            } else {
                json!({"items":[],"nextCursor":null})
            };
            return Some((200, tracker.to_string()));
        }
        None
    })
}

#[gpui_kit::test]
fn new_subissue_on_empty_tracker_prefills_and_returns_to_the_parent(cx: &mut TestAppContext) {
    let fixture = empty_tracker();
    let (board, cx) = opened(&fixture, cx);
    assert_eq!(
        label(cx, "issue-new-subissue").as_deref(),
        Some("New sub-issue")
    );
    assert!(!has(cx, "issue-child:4"));
    cx.update(|w, cx| w.click("issue-new-subissue", cx));
    render(cx);
    assert!(has(cx, "issue-editor-root"));
    assert!(labelled(cx, "Parent: SUB-1"));
    assert!(!cx.update(|_, cx| board.read(cx).issue_editor_dirty(cx)));
    cx.update(|w, cx| {
        w.input("A new sub-issue", cx);
        w.press("tab", cx);
        w.input("Work from the tracker", cx);
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
    settle_board(&board, cx, |b| b.editor.is_none() && super::settled(b));
    render(cx);
    assert!(has(cx, "issue-child:4"));
    assert!(has(cx, "issue-child:3"));
    assert!(has(cx, "issue-more-subissues"));
    let writes: Vec<Value> = fixture
        .mock
        .requests()
        .iter()
        .filter(|r| r.method == "POST")
        .map(|r| serde_json::from_str(&r.body).unwrap())
        .collect();
    assert_eq!(
        writes,
        [
            json!({"projectId":"project-SUB","parentId":"SUB-1","summary":"Work from the tracker","title":"A new sub-issue"})
        ]
    );
}

#[gpui_kit::test]
fn refresh_checks_projected_children_without_paging(cx: &mut TestAppContext) {
    let fixture = offline_with(|request| {
        let id = request.path.strip_prefix("/api/v1/issues/")?;
        let mut child = row(id, "Renamed child");
        child["parentId"] = json!("1");
        child["todos"] = json!([]);
        match id {
            "4" => child["parentId"] = json!("2"),
            "5" => child["archivedAt"] = json!("2026-11-10T10:00:00Z"),
            "6" => child["state"] = json!({"id":"state-done","name":"Done","type":"completed"}),
            _ => return None,
        }
        Some((200, child.to_string()))
    });
    let (board, cx) = opened(&fixture, cx);
    board.update(cx, |board, cx| {
        for id in ["4", "5", "6"] {
            let mut fields = row(id, "New child");
            fields["parentId"] = json!("1");
            board
                .issue_view
                .project_subissue(serde_json::from_value(fields).unwrap());
        }
        board.reload_detail(cx);
    });
    settle_board(&board, cx, |board| {
        let rows: Vec<_> = board.issue_view.projected_subissues().collect();
        rows.len() == 1
            && rows[0].id == "6"
            && rows[0].title == "Renamed child"
            && rows[0].state.name == "Done"
    });
    render(cx);
    assert!(has(cx, "issue-more-subissues"));
    assert!(!has(cx, "issue-child:4"));
    assert!(!has(cx, "issue-child:5"));
    assert!(labelled(cx, "SUB-6, Done, Renamed child"));
    let requests = fixture.mock.requests();
    for id in ["4", "5", "6"] {
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.path == format!("/api/v1/issues/{id}"))
                .count(),
            1
        );
    }
    assert!(
        !requests
            .iter()
            .any(|r| r.path.contains("cursor=children-second"))
    );
}

#[gpui_kit::test]
fn temporary_child_read_failure_keeps_the_row_until_the_next_refresh(cx: &mut TestAppContext) {
    const CHILD: &str = "/api/v1/issues/4";
    let fail = Arc::new(AtomicBool::new(true));
    let unavailable = fail.clone();
    let fixture = offline_with(move |request| {
        if request.path != CHILD {
            return None;
        }
        if unavailable.load(Ordering::SeqCst) {
            return Some((503, "{}".into()));
        }
        let mut child = row("4", "Renamed child");
        child["parentId"] = json!("1");
        child["todos"] = json!([]);
        Some((200, child.to_string()))
    });
    let (board, cx) = opened(&fixture, cx);
    board.update(cx, |board, cx| {
        let mut child = row("4", "New child");
        child["parentId"] = json!("1");
        board
            .issue_view
            .project_subissue(serde_json::from_value(child).unwrap());
        board.reload_detail(cx);
    });
    settle_board(&board, cx, |board| {
        board.issue_view.subissue_projection_checked("4")
    });
    render(cx);
    assert!(has(cx, "issue-more-subissues"));
    assert!(labelled(cx, "SUB-4, In Progress, New child"));
    let reads = || {
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.path == CHILD)
            .count()
    };
    assert_eq!(reads(), 1);

    fail.store(false, Ordering::SeqCst);
    board.update(cx, |board, cx| board.reload_detail(cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .projected_subissues()
            .any(|row| row.title == "Renamed child")
    });
    render(cx);
    assert!(has(cx, "issue-more-subissues"));
    assert!(labelled(cx, "SUB-4, In Progress, Renamed child"));
    assert_eq!(reads(), 2);
}
