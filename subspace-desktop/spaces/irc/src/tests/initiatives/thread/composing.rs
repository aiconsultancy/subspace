//! The inspector and channel editors, with independent draft and send paths.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::Command;

use super::{MESSAGE_TAGS, SHIP, channel, finished, front, settle};
use crate::conversation::Conversation;

pub(crate) fn reply_input(
    conversation: &Entity<Conversation>,
    root: &str,
    cx: &mut VisualTestContext,
) -> Entity<subspace_framework::dashboard::ChatInput> {
    conversation.read_with(cx, |view, cx| {
        view.reply_drafts
            .get(root)
            .unwrap()
            .read(cx)
            .input()
            .clone()
    })
}

#[gpui_kit::test]
fn reply_in_thread_sends_to_the_channel_with_the_reply_tag(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    let conversation = front(&board, cx);
    let channel_input = conversation.read_with(cx, |view, _| view.composer().clone());
    let input = reply_input(&conversation, "ask1", cx);
    cx.update(|window, cx| {
        assert!(
            window
                .within("workspace-inspector-footer")
                .find("irc-reply-editor")
                .visible()
        );
        channel_input.update(cx, |input, cx| input.set_text("Channel draft", window, cx));
        input.update(cx, |input, cx| input.set_text("Thanks, Lyra.", window, cx));
    });
    settle(cx);
    cx.update(|window, cx| window.within("irc-reply-editor").click("message-send", cx));
    settle(cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::Reply {
                buffer: channel(SHIP),
                msgid: "ask1".into(),
                text: "Thanks, Lyra.".into(),
            })
        );
    });
    input.read_with(cx, |input, cx| assert!(input.text(cx).is_empty()));
    channel_input.read_with(cx, |input, cx| assert_eq!(input.text(cx), "Channel draft"));
}

#[gpui_kit::test]
fn changing_threads_and_reopening_the_inspector_keep_independent_drafts(cx: &mut TestAppContext) {
    let (board, _, cx) = finished(cx, &[MESSAGE_TAGS]);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    let conversation = front(&board, cx);
    let input = reply_input(&conversation, "ask1", cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text("Draft for first", window, cx)
        })
    });
    settle(cx);
    cx.update(|window, cx| window.click("workspace-inspector-close", cx));
    settle(cx);
    cx.update(|window, cx| window.click("irc-resume-reply", cx));
    settle(cx);
    assert_eq!(reply_input(&conversation, "ask1", cx), input);
    board.update_in(cx, |board, window, cx| {
        crate::thread::open(board, &channel(SHIP), "r1".into(), window, cx)
    });
    settle(cx);
    let other = reply_input(&conversation, "r1", cx);
    other.read_with(cx, |input, cx| assert!(input.text(cx).is_empty()));
    board.update_in(cx, |board, window, cx| {
        crate::thread::open(board, &channel(SHIP), "ask1".into(), window, cx)
    });
    settle(cx);
    input.read_with(cx, |input, cx| {
        assert_eq!(input.text(cx), "Draft for first")
    });
}
