//! The send hint: its wording, fading with text, and when errors or blocked
//! sending replace it.

use super::ChatInput;
use super::tests::submissions;
use crate::ChatInputEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use std::time::Duration;

#[test]
fn the_hint_names_the_platform_s_send_and_new_line_keys() {
    use super::send_hint;
    assert_eq!(
        send_hint(false, true),
        "Return to send · Shift+Return for a new line"
    );
    assert_eq!(
        send_hint(false, false),
        "Enter to send · Shift+Enter for a new line"
    );
    assert_eq!(
        send_hint(true, true),
        "Markdown · Return to send · Shift+Return for a new line"
    );
}

fn finish_hint(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.render_frame(cx));
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.update(|window, cx| window.render_frame(cx));
}

#[gpui_kit::test]
fn the_send_hint_fades_with_text_and_returns_after_clearing(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        assert!(window.find("message-hint").visible());
        window.input(" ", cx);
    });
    cx.run_until_parked();
    finish_hint(cx);
    cx.update(|window, _| assert!(!window.find("message-hint").visible()));
    cx.update(|window, cx| input.update(cx, |input, cx| input.clear(window, cx)));
    finish_hint(cx);
    cx.update(|window, _| assert!(window.find("message-hint").visible()));
}

#[test]
fn hint_opacity_hides_text_and_blocked_submission() {
    use super::hint_opacity;
    assert_eq!(hint_opacity(false, false), 1.0);
    assert_eq!(hint_opacity(true, false), 0.0);
    assert_eq!(hint_opacity(false, true), 0.0);
    assert_eq!(hint_opacity(true, true), 0.0);
}

#[gpui_kit::test]
fn the_hint_hides_while_submission_is_blocked(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    finish_hint(cx);
    cx.update(|window, _| assert!(window.find("message-hint").visible()));
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_submit_blocked(true, cx)));
    finish_hint(cx);
    cx.update(|window, _| assert!(!window.find("message-hint").visible()));
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_submit_blocked(false, cx)));
    finish_hint(cx);
    cx.update(|window, _| assert!(window.find("message-hint").visible()));
}

#[gpui_kit::test]
fn an_error_replaces_the_hint(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    finish_hint(cx);
    cx.update(|window, _| assert!(window.find("message-hint").visible()));
    cx.update(|_, cx| {
        input.update(cx, |input, cx| {
            input.set_error("Fixture send failure".into(), cx)
        })
    });
    finish_hint(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("message-hint").is_none());
        assert_eq!(
            input.read(cx).message.as_ref().unwrap().error.as_deref(),
            Some("Fixture send failure")
        );
    });
}

#[gpui_kit::test]
fn the_hint_returns_when_the_owner_clears_a_sent_draft(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (input, cx) = cx.add_window_view(ChatInput::message);
    let (submitted, _recording) = submissions(&input, cx);
    let _sending = cx.update(|window, cx| {
        let handle = window.window_handle();
        cx.subscribe(&input, move |input, event, cx| {
            if matches!(event, ChatInputEvent::Submit(_)) {
                cx.update_window(handle, |_, window, cx| {
                    input.update(cx, |input, cx| input.clear(window, cx));
                })
                .unwrap();
            }
        })
    });
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
        window.input("Fixture message", cx);
    });
    cx.run_until_parked();
    finish_hint(cx);
    cx.update(|window, _| assert!(!window.find("message-hint").visible()));
    cx.update(|window, cx| window.press("enter", cx));
    cx.run_until_parked();
    finish_hint(cx);
    assert_eq!(&*submitted.borrow(), &["Fixture message"]);
    cx.update(|window, cx| {
        assert!(input.read(cx).text(cx).is_empty());
        assert!(window.find("message-hint").visible());
    });
}
