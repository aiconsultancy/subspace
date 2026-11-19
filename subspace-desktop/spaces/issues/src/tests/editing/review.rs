use super::{create_and_submit, issue, open_editor, submit_text};
use crate::tests::{
    has, label,
    offline::{offline_with, row, settle_board},
    render,
};
use gpui_kit::{TestAppContext, test::TestWindowExt as _};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_api_client::issue_writes::IssueSaveError;

#[gpui_kit::test]
fn several_create_matches_keep_the_editor_frozen_and_submit_disabled(cx: &mut TestAppContext) {
    let posted = Arc::new(AtomicBool::new(false));
    let sent = posted.clone();
    let fixture = offline_with(move |r| {
        if r.method == "POST" {
            sent.store(true, Ordering::SeqCst);
            return Some((500, "{}".into()));
        }
        if r.path.contains("creator=me") {
            let items = if sent.load(Ordering::SeqCst) {
                vec![
                    row("new-1", "Unclear fixture"),
                    row("new-2", "Unclear fixture"),
                ]
            } else {
                vec![]
            };
            return Some((200, json!({"items":items,"nextCursor":null}).to_string()));
        }
        None
    });
    let (board, cx) = create_and_submit(&fixture, cx, "Unclear fixture");
    cx.update(|w, cx| w.click("issue-editor-check", cx));
    settle_board(&board, cx, |b| b.editor.as_ref().is_some_and(|e| !e.busy()));
    render(cx);
    cx.update(|w, _| assert_eq!(w.find("issue-editor-save").disabled(), Some(true)));
    cx.update(|w, cx| {
        w.click("issue-editor-save", cx);
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
        w.input(" extra", cx);
    });
    render(cx);
    cx.update(|w, _| assert_eq!(w.find("issue-edit-title").value(), Some("Unclear fixture")));
    assert_eq!(
        label(cx, "issue-editor-notice"),
        Some(IssueSaveError::SeveralMatches.to_string())
    );
    assert!(has(cx, "issue-editor-notice"));
    assert_eq!(
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
}

#[gpui_kit::test]
fn archived_create_shows_a_project_action_and_preserves_input(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        if r.method == "POST" {
            return Some((409, "{}".into()));
        }
        if r.path.contains("creator=me") {
            return Some((200, json!({"items":[],"nextCursor":null}).to_string()));
        }
        None
    });
    let (_board, cx) = create_and_submit(&fixture, cx, "Retained fixture");
    assert!(has(cx, "issue-editor-notice"));
    assert!(!has(cx, "issue-editor-reload-version"));
    cx.update(|w, _| assert_eq!(w.find("issue-edit-title").value(), Some("Retained fixture")));
    assert_eq!(
        label(cx, "issue-editor-notice"),
        Some(IssueSaveError::ProjectArchived.to_string())
    );
}

#[gpui_kit::test]
fn archived_patch_does_not_offer_version_recovery(cx: &mut TestAppContext) {
    let patched = Arc::new(AtomicBool::new(false));
    let sent = patched.clone();
    let fixture = offline_with(move |r| {
        if r.method == "PATCH" {
            sent.store(true, Ordering::SeqCst);
            return Some((409, "{}".into()));
        }
        if matches!(r.path.as_str(), "/api/v1/issues/2" | "/api/v1/issues/SUB-2") {
            let mut detail: serde_json::Value =
                serde_json::from_str(&issue("Original", 1)).unwrap();
            if sent.load(Ordering::SeqCst) {
                detail["archivedAt"] = json!("2026-11-16T12:00:00Z");
            }
            return Some((200, detail.to_string()));
        }
        None
    });
    let (board, cx) = open_editor(&fixture, cx);
    submit_text(cx, "My edit");
    settle_board(&board, cx, |b| b.editor.as_ref().is_some_and(|e| !e.busy()));
    render(cx);
    assert!(has(cx, "issue-editor-notice"));
    assert!(!has(cx, "issue-editor-reload-version"));
    assert!(!has(cx, "issue-editor-use-version"));
    assert_eq!(
        label(cx, "issue-editor-notice"),
        Some(IssueSaveError::IssueArchived.to_string())
    );
}
