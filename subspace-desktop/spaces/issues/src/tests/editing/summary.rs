use super::{CREATE_SUMMARY, open_editor, patch_bodies};
use crate::tests::{
    has,
    offline::{board_with_hub, offline_with, row, settle_board},
    render,
};
use gpui_kit::{TestAppContext, test::TestWindowExt as _};
use serde_json::{Value, json};
use subspace_api_client::issue_writes::SUMMARY_LIMIT;

#[gpui_kit::test]
fn create_refuses_missing_and_overlong_summary_and_sends_a_valid_single_line(
    cx: &mut TestAppContext,
) {
    let fixture = offline_with(|r| (r.method == "POST").then(|| (201, super::issue("Fixture", 1))));
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    render(cx);
    cx.update(|w, cx| {
        w.press("c", cx);
        assert!(w.try_find("issue-summary-error").is_none());
        w.input("Fixture", cx);
        assert!(w.try_find("issue-summary-error").is_none());
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
    render(cx);
    assert!(has(cx, "issue-edit-summary"));
    let message = crate::tests::label(cx, "issue-summary-error").unwrap();
    assert_eq!(
        message,
        "Add a summary of the problem this issue solves (up to 300 characters)."
    );
    assert!(
        !crate::tests::label(cx, "issue-editor-notice")
            .unwrap()
            .contains("cannot be cleared")
    );
    assert!(!fixture.mock.requests().iter().any(|r| r.method == "POST"));
    for text in ["x".repeat(SUMMARY_LIMIT + 1), CREATE_SUMMARY.into()] {
        cx.update(|w, cx| {
            w.click("issue-edit-summary", cx);
            w.press(subspace_framework::dashboard::secondary!("a"), cx);
            w.input(&text, cx);
            w.press(subspace_framework::dashboard::secondary!("enter"), cx);
        });
        render(cx);
        if text.len() > SUMMARY_LIMIT {
            assert!(!fixture.mock.requests().iter().any(|r| r.method == "POST"));
            cx.update(|w, _| assert_eq!(w.find("issue-editor-save").disabled(), Some(true)));
        }
    }
    settle_board(&board, cx, |b| b.editor.is_none());
    let post = fixture
        .mock
        .requests()
        .into_iter()
        .find(|r| r.method == "POST")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&post.body).unwrap()["summary"],
        CREATE_SUMMARY
    );
}

#[gpui_kit::test]
fn page_summary_inline_edit_refuses_clearing_and_cancel_keeps_the_value(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        (r.path == "/api/v1/issues/SUB-2" || r.path == "/api/v1/issues/2").then(|| {
            let mut value = row("2", "Summary fixture");
            value["todos"] = json!([]);
            value["summary"] = json!(CREATE_SUMMARY);
            (200, value.to_string())
        })
    });
    let (board, cx) = open_editor(&fixture, cx);
    cx.update(|w, cx| w.press("escape", cx));
    render(cx);
    assert_eq!(
        crate::tests::label(cx, "issue-summary-content").as_deref(),
        Some(CREATE_SUMMARY)
    );
    cx.update(|w, cx| w.click("issue-summary-edit", cx));
    render(cx);
    assert!(!has(cx, "issue-summary-error"));
    cx.update(|w, cx| {
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.press("backspace", cx);
    });
    render(cx);
    assert!(
        crate::tests::label(cx, "issue-summary-error")
            .unwrap()
            .contains("cannot be cleared")
    );
    cx.update(|w, cx| {
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
    render(cx);
    assert!(patch_bodies(&fixture).is_empty());
    assert!(
        crate::tests::label(cx, "issue-editor-notice")
            .unwrap()
            .contains("cannot be cleared")
    );
    cx.update(|w, cx| w.click("issue-inline-cancel", cx));
    render(cx);
    assert!(cx.update(|_, cx| board.read(cx).editor.is_none()));
}

#[gpui_kit::test]
fn legacy_page_shows_no_summary_and_can_save_its_first_summary(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        if !matches!(r.path.as_str(), "/api/v1/issues/SUB-2" | "/api/v1/issues/2") {
            return None;
        }
        let mut value = row("2", "Legacy issue");
        value["todos"] = json!([]);
        value["summary"] = if r.method == "PATCH" {
            json!(CREATE_SUMMARY)
        } else {
            Value::Null
        };
        Some((200, value.to_string()))
    });
    let (board, cx) = open_editor(&fixture, cx);
    assert!(!has(cx, "issue-summary-error"));
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    render(cx);
    assert_eq!(
        crate::tests::label(cx, "issue-summary-error").as_deref(),
        Some("Add a summary of the problem this issue solves (up to 300 characters).")
    );
    assert!(patch_bodies(&fixture).is_empty());
    cx.update(|w, cx| w.press("escape", cx));
    render(cx);
    assert!(crate::tests::labelled(cx, "No summary yet"));
    cx.update(|w, cx| w.click("issue-summary-edit", cx));
    render(cx);
    assert!(!has(cx, "issue-summary-error"));
    cx.update(|w, cx| {
        w.input("x", cx);
        w.press("backspace", cx);
    });
    render(cx);
    assert_eq!(
        crate::tests::label(cx, "issue-summary-error").as_deref(),
        Some("Add a summary of the problem this issue solves (up to 300 characters).")
    );
    cx.update(|w, cx| {
        w.input(CREATE_SUMMARY, cx);
        w.press(subspace_framework::dashboard::secondary!("enter"), cx);
    });
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"summary":CREATE_SUMMARY})]
    );
}

#[gpui_kit::test]
fn new_summary_shows_validation_after_editing_and_counts_trimmed_utf16(cx: &mut TestAppContext) {
    let fixture = offline_with(|_| None);
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    render(cx);
    cx.update(|w, cx| {
        w.press("c", cx);
        w.click("issue-edit-summary", cx);
        w.input("🙂", cx);
    });
    render(cx);
    assert!(!has(cx, "issue-summary-error"));
    cx.update(|w, cx| w.press("backspace", cx));
    render(cx);
    assert!(has(cx, "issue-summary-error"));
    cx.update(|w, cx| w.input(&"🙂".repeat(SUMMARY_LIMIT), cx));
    render(cx);
    assert!(has(cx, "issue-summary-error"));
    cx.update(|w, cx| {
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.input("  A valid summary  ", cx);
    });
    render(cx);
    assert!(!has(cx, "issue-summary-error"));
    assert!(!fixture.mock.requests().iter().any(|r| r.method == "POST"));
}
