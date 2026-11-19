use subspace_agent::failure::TurnContext;
use subspace_agent::{FailureCategory, TurnFailure};
use subspace_core::diagnostics::LocalDiagnostics;

use super::error_details;
use super::tests::{settle, setup};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

#[test]
fn local_copy_keeps_the_recorded_time_and_scrubs_submitted_text() {
    let mut turn = TurnContext::default();
    turn.begin(0, vec!["PRIVATE FIXTURE PROMPT".into()]);
    let mut failure = TurnFailure::stopped(
        FailureCategory::Stopped,
        "Rejected PRIVATE FIXTURE PROMPT",
        &turn,
        Some("fixture-runtime"),
    );
    failure.occurred_at = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05+00:00")
        .unwrap()
        .with_timezone(&chrono::Local);
    let projection = error_details(&failure);
    let copied = LocalDiagnostics::default().local_copy(Some(&projection));
    let text = copied.markdown();
    assert!(text.contains("2026-01-02T03:04:05"), "{text}");
    assert!(text.contains("Occurred at (UTC)"), "{text}");
    assert!(!text.contains("Observed at (UTC)"), "{text}");
    assert!(text.contains("[message text removed]"), "{text}");
    assert!(!text.contains("PRIVATE FIXTURE PROMPT"), "{text}");
    assert!(text.contains("fixture-runtime"), "{text}");
    assert!(text.contains("Copilot turn"), "{text}");
}

#[gpui_kit::test]
fn legacy_startup_error_copies_diagnostics_and_keeps_reconnect(cx: &mut TestAppContext) {
    let (chat, cx) = setup(cx);
    chat.update(cx, |chat, _| {
        chat.connected(Err("Fixture runtime unavailable".into()));
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-reconnect").visible());
        assert_eq!(window.find("chat-error-copy").label(), Some("Copy details"));
        window.click("chat-error-copy", cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.find("chat-reconnect").visible());
        assert_eq!(window.find("chat-error-copy").label(), Some("Copy details"));
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap();
        assert!(copied.contains("Fixture runtime unavailable"), "{copied}");
        assert!(copied.contains("Observed at (UTC)"), "{copied}");
        assert!(copied.contains("## Diagnostics"), "{copied}");
    });
}
