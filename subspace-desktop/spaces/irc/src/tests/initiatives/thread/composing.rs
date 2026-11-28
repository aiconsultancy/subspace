//! The inspector and channel editors, with independent draft and send paths.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{Command, Event};

use super::{MESSAGE_TAGS, SHIP, channel, finished, front, inject, network_id, settle};
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
    cx.update(|window, cx| window.click("workspace-inspector-close", cx));
    settle(cx);
    cx.update(|window, cx| window.click("irc-resume-reply", cx));
    settle(cx);
    assert_eq!(reply_input(&conversation, "ask1", cx), input);
    board.read_with(cx, |view, _| {
        assert_eq!(view.threads.chosen(&channel(SHIP)), Some("ask1"));
    });
    input.read_with(cx, |input, cx| {
        assert_eq!(input.text(cx), "Draft for first")
    });
}

#[gpui_kit::test]
fn replying_from_the_transcript_focuses_the_inspector_and_keeps_the_draft_through_cap_changes(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    let conversation = front(&board, cx);
    let room = channel(SHIP);
    let original = hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&room)
            .unwrap()
            .entries
            .iter()
            .find(|entry| entry.line.id.as_deref() == Some("ask1"))
            .unwrap()
            .seq
    });
    conversation.update_in(cx, |view, window, cx| {
        crate::conversation::reply_in_thread(view, original, window, cx);
    });
    settle(cx);
    cx.simulate_input("Ready for review.");
    settle(cx);
    let input = reply_input(&conversation, "ask1", cx);
    input.read_with(cx, |input, cx| {
        assert_eq!(input.text(cx), "Ready for review.")
    });
    conversation.read_with(cx, |view, cx| {
        assert!(view.composer().read(cx).text(cx).is_empty())
    });

    inject(&hub, vec![capabilities(vec![])], cx);
    cx.update(|window, _| assert!(window.find("irc-thread-reply-refused").visible()));
    input.read_with(cx, |input, cx| {
        assert_eq!(input.text(cx), "Ready for review.")
    });
    inject(&hub, vec![capabilities(vec![MESSAGE_TAGS.into()])], cx);
    cx.update(|window, cx| window.within("irc-reply-editor").click("message-send", cx));
    settle(cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::Reply {
                buffer: room,
                msgid: "ask1".into(),
                text: "Ready for review.".into(),
            })
        );
    });
}

fn capabilities(enabled: Vec<String>) -> Event {
    Event::Capabilities {
        network: network_id(),
        enabled,
    }
}
