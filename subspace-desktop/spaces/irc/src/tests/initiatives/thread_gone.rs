//! A reply whose thread is no longer held, as under the line cap: kept in
//! reply mode, sent nowhere, until the person chooses to write to the
//! channel instead.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::super::remove::settle;
use super::thread::{finished, front};
use subspace_framework::dashboard::ChatInputEvent;

use crate::thread::MESSAGE_TAGS;

#[gpui_kit::test]
fn a_reply_whose_thread_goes_is_kept_and_sent_nowhere_until_chosen(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    cx.update(|window, cx| window.click("irc-thread-reply", cx));
    settle(cx);
    let conversation = front(&board, cx);
    // The root is no longer held, as under the line cap.
    conversation.update(cx, |view, cx| {
        crate::compose::reply::forget(view, "ask1", cx)
    });
    settle(cx);
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Submit("/part".into())));
    // Escape keeps the guard: the next Enter still sends nothing.
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Cancel));
    settle(cx);
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Submit("/part".into())));
    settle(cx);
    assert_eq!(
        hub.read_with(cx, |hub, _| hub.sent().len()),
        before,
        "nothing sent"
    );
    conversation.read_with(cx, |conversation, _| {
        assert!(
            conversation
                .draft
                .reply
                .as_ref()
                .is_some_and(|reply| reply.gone)
        );
    });
    cx.update(|window, cx| window.click("irc-reply-mode-to-channel", cx));
    settle(cx);
    conversation.read_with(cx, |conversation, _| {
        assert_eq!(conversation.draft.reply, None)
    });
}
