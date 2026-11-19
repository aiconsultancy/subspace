//! The draft against a change made elsewhere: Reload is offered, nothing moves.
use super::{issue, open_editor, patch_bodies};
use crate::tests::{
    has,
    offline::{offline_with, settle_board},
    render,
};
use gpui_kit::{TestAppContext, test::TestWindowExt as _};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[gpui_kit::test]
fn a_change_elsewhere_offers_reload_without_touching_the_draft(cx: &mut TestAppContext) {
    let version = Arc::new(AtomicUsize::new(1));
    let current = version.clone();
    let fixture = offline_with(move |r| {
        if r.method == "PATCH" && r.path == "/api/v1/issues/2" {
            return Some((200, issue("My edit", 3)));
        }
        if r.path == "/api/v1/issues/2" || r.path == "/api/v1/issues/SUB-2" {
            let at = current.load(Ordering::SeqCst);
            let mut value: Value = serde_json::from_str(&issue("Original", at as u32)).unwrap();
            if at > 1 {
                value["description"] = json!("Their description");
            }
            return Some((200, value.to_string()));
        }
        None
    });
    let (board, cx) = open_editor(&fixture, cx);
    cx.update(|w, cx| {
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.input("My edit", cx);
    });
    render(cx);
    assert!(!has(cx, "issue-editor-reload-elsewhere"));
    // Someone else saves; the hub reads the issue again, as a live hint does.
    version.store(2, Ordering::SeqCst);
    board.update(cx, |b, cx| b.reload_detail(cx));
    settle_board(&board, cx, |b| b.issue_changed_elsewhere());
    render(cx);
    assert!(has(cx, "issue-editor-reload-elsewhere"));
    assert!(cx.update(|_, cx| board.read(cx).issue_editor_dirty(cx)));
    cx.update(|w, cx| w.click("issue-editor-reload-elsewhere", cx));
    render(cx);
    assert!(!has(cx, "issue-editor-reload-elsewhere"));
    cx.update(|w, cx| w.click("issue-editor-save", cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    // The draft rebased on their version and kept only the person's title.
    assert_eq!(
        patch_bodies(&fixture),
        vec![json!({"version":2,"title":"My edit"})]
    );
}
