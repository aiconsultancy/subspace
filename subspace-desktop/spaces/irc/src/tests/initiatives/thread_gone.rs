//! A reply draft outlives the original message in the loaded transcript.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_irc::Event;
use subspace_framework::dashboard::ChatInputEvent;

use super::super::{inject, remove::settle};
use super::thread::{finished, front, reply_input};
use crate::fixtures::{channel, message};
use crate::thread::MESSAGE_TAGS;

#[gpui_kit::test]
fn an_evicted_root_keeps_a_copyable_draft_and_refuses_send(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    let conversation = front(&board, cx);
    let input = reply_input(&conversation, "ask1", cx);
    let words = "Ready for review.";
    cx.update(|window, cx| input.update(cx, |input, cx| input.set_text(words, window, cx)));
    settle(cx);
    let room = channel("#aurora-lyra");
    inject(
        &hub,
        (0..crate::state::LINE_CAP)
            .map(|i| Event::Line(message(&room, "mira", "New history", i as i64 + 100)))
            .collect(),
        cx,
    );
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Submit(words.into())));
    settle(cx);
    assert_eq!(hub.read_with(cx, |hub, _| hub.sent().len()), before);
    input.read_with(cx, |input, cx| assert_eq!(input.text(cx), words));
    cx.update(|window, cx| {
        assert!(window.find("irc-unavailable-replies").visible());
        window.click("irc-copy-reply:ask1", cx);
    });
    settle(cx);
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some(words)
        )
    });
}
