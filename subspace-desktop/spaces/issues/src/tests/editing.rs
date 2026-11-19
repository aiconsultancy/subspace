mod details;
mod elsewhere;
mod properties;
mod review;
mod summary;
const CREATE_SUMMARY: &str = "Explain the work so people can decide what to do next.";
use super::offline::{Offline, board_with_hub, offline_with, row, settle_board};
use super::{has, render};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use subspace_framework::client_test_support::Request;
use subspace_framework::dashboard::Dashboard as _;

fn issue(title: &str, version: u32) -> String {
    let mut value = row("2", title);
    value["todos"] = json!([]);
    value["description"] = json!("@[Ada Fixture](member:12345678-1234-1234-1234-123456789abc)");
    value["version"] = json!(version);
    value.to_string()
}

fn open_editor<'a>(
    fixture: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<crate::Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    open_page(&board, cx);
    cx.update(|window, cx| window.press("e", cx));
    render(cx);
    assert!(has(cx, "issue-editor-root"));
    (board, cx)
}

fn open_page(board: &Entity<crate::Issues>, cx: &mut VisualTestContext) {
    board.update(cx, |b, cx| b.open_detail("SUB-2".into(), cx));
    settle_board(board, cx, |b| b.can_edit_issue());
    render(cx);
}

#[gpui_kit::test]
fn create_from_toolbar_and_submit_from_keyboard_opens_saved_issue(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        if r.method == "POST" && r.path == "/api/v1/issues" {
            return Some((201, issue("Created fixture", 1)));
        }
        if r.path.starts_with("/api/v1/issues?") && r.path.contains("creator=me") {
            return Some((200, json!({"items":[],"nextCursor":null}).to_string()));
        }
        None
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    render(cx);
    cx.update(|w, cx| w.click("issues-create", cx));
    render(cx);
    cx.update(|w, cx| {
        w.input("Created fixture", cx);
        w.press("tab", cx);
        w.input(CREATE_SUMMARY, cx);
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
    settle_board(&board, cx, |b| {
        b.editor.is_none()
            && b.issue_view
                .snapshot
                .as_ref()
                .and_then(|s| s.detail())
                .is_some()
    });
    let posts: Vec<_> = fixture
        .mock
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .collect();
    assert_eq!(posts.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&posts[0].body).unwrap(),
        json!({"projectId":"project-SUB","title":"Created fixture","summary":CREATE_SUMMARY})
    );
}

#[gpui_kit::test]
fn conflict_requires_reload_and_preserves_only_the_fields_the_person_changed(
    cx: &mut TestAppContext,
) {
    let patches = Arc::new(AtomicUsize::new(0));
    let count = patches.clone();
    let fixture = offline_with(move |r| conflict_route(r, &count));
    let (board, cx) = open_editor(&fixture, cx);
    cx.update(|w, cx| {
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.input("My edit", cx);
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
    settle_board(&board, cx, |b| b.editor.as_ref().is_some_and(|e| !e.busy()));
    render(cx);
    assert!(has(cx, "issue-editor-reload-version"));
    cx.update(|w, cx| w.click("issue-editor-reload-version", cx));
    settle_board(&board, cx, |b| b.editor.as_ref().is_some_and(|e| !e.busy()));
    render(cx);
    cx.update(|w, cx| w.click("issue-editor-use-version", cx));
    render(cx);
    cx.update(|w, cx| w.click("issue-editor-save", cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        vec![
            json!({"version":1,"title":"My edit"}),
            json!({"version":2,"title":"My edit"})
        ]
    );
}

fn conflict_route(r: &Request, patches: &AtomicUsize) -> Option<(u16, String)> {
    if r.method == "PATCH" && r.path == "/api/v1/issues/2" {
        return Some(if patches.fetch_add(1, Ordering::SeqCst) == 0 {
            (409, json!({"currentVersion":2}).to_string())
        } else {
            (200, issue("My edit", 3))
        });
    }
    if r.path == "/api/v1/issues/2" || r.path == "/api/v1/issues/SUB-2" {
        return Some((
            200,
            issue(
                "Original",
                if patches.load(Ordering::SeqCst) == 0 {
                    1
                } else {
                    2
                },
            ),
        ));
    }
    None
}

#[gpui_kit::test]
fn escape_confirms_discard_and_restores_focus_without_sending(cx: &mut TestAppContext) {
    let fixture = offline_with(|_| None);
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    render(cx);
    cx.update(|w, cx| w.press("c", cx));
    render(cx);
    assert!(has(cx, "issue-editor-root"));
    cx.update(|w, cx| {
        w.input("Unsaved fixture", cx);
        w.press("escape", cx);
    });
    render(cx);
    assert!(
        cx.update(|_, cx| board.read(cx).issue_editor_dirty(cx)),
        "draft should retain the title"
    );
    assert!(has(cx, "issue-editor-discard"));
    cx.update(|w, cx| w.click("issue-editor-keep", cx));
    render(cx);
    assert!(has(cx, "issue-editor-root"));
    cx.update(|w, cx| w.click("issue-editor-cancel", cx));
    render(cx);
    cx.update(|w, cx| w.click("issue-editor-discard-now", cx));
    render(cx);
    assert!(cx.update(|_, cx| board.read(cx).editor.is_none()));
    assert!(!fixture.mock.requests().iter().any(|r| r.method == "POST"));
}

#[gpui_kit::test]
fn unclear_create_freezes_text_and_keyboard_submit_until_checked(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        if r.method == "POST" {
            return Some((500, "{}".into()));
        }
        if r.path.starts_with("/api/v1/issues?") && r.path.contains("creator=me") {
            return Some((200, json!({"items":[],"nextCursor":null}).to_string()));
        }
        None
    });
    let (board, cx) = create_and_submit(&fixture, cx, "Unclear fixture");
    assert!(has(cx, "issue-editor-check"));
    cx.update(|w, cx| {
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
        w.input(" extra", cx);
    });
    render(cx);
    assert_eq!(
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
    cx.update(|w, cx| w.click("issue-editor-check", cx));
    settle_board(&board, cx, |b| b.editor.as_ref().is_some_and(|e| !e.busy()));
    render(cx);
    cx.update(|w, cx| w.click("issue-editor-save", cx));
    settle_board(&board, cx, |b| b.editor.as_ref().is_some_and(|e| !e.busy()));
    let bodies: Vec<Value> = fixture
        .mock
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .map(|r| serde_json::from_str(&r.body).unwrap())
        .collect();
    assert_eq!(bodies.len(), 2);
    assert!(bodies.iter().all(|b| b["title"] == "Unclear fixture"));
}

#[gpui_kit::test]
fn hiding_and_following_a_reference_returns_to_the_retained_edit(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        (r.path == "/api/v1/issues/SUB-2" || r.path == "/api/v1/issues/2")
            .then(|| (200, issue("Original", 1)))
    });
    let (board, cx) = open_editor(&fixture, cx);
    cx.update(|w, cx| {
        w.input("Retained draft", cx);
        board.update(cx, |b, cx| {
            b.deactivate(w, cx);
            b.activate(w, cx);
        });
    });
    settle_board(&board, cx, |b| b.can_edit_issue());
    render(cx);
    assert!(has(cx, "issue-editor-root"));
    assert!(cx.update(|_, cx| !board.read(cx).quit_warnings(cx).is_empty()));
}

fn patch_bodies(fixture: &Offline) -> Vec<Value> {
    fixture
        .mock
        .requests()
        .into_iter()
        .filter(|r| r.method == "PATCH")
        .map(|r| serde_json::from_str(&r.body).unwrap())
        .collect()
}

fn create_and_submit<'a>(
    fixture: &Offline,
    cx: &'a mut TestAppContext,
    title: &str,
) -> (Entity<crate::Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    render(cx);
    cx.update(|w, cx| w.press("c", cx));
    render(cx);
    cx.update(|w, cx| {
        w.input(title, cx);
        w.press("tab", cx);
        w.input(CREATE_SUMMARY, cx);
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
    settle_board(&board, cx, |b| b.editor.as_ref().is_some_and(|e| !e.busy()));
    render(cx);
    (board, cx)
}

fn submit_text(cx: &mut VisualTestContext, text: &str) {
    cx.update(|w, cx| {
        w.input(text, cx);
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
}
