//! A reply whose thread is no longer held, as under the line cap: kept in
//! reply mode, sent nowhere, until the person chooses to write to the
//! channel instead, whatever else they do meanwhile.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::ChatInputEvent;

use super::super::remove::settle;
use super::thread::{finished, front};
use crate::Irc;
use crate::conversation::Conversation;
use crate::hub::Hub;
use crate::thread::MESSAGE_TAGS;

/// The ship's conversation in reply mode on `ask1`, whose root then goes,
/// as under the line cap.
fn gone_reply(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Entity<Conversation> {
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    cx.update(|window, cx| window.click("irc-thread-reply", cx));
    settle(cx);
    let conversation = front(board, cx);
    conversation.update(cx, |view, cx| {
        crate::compose::reply::forget(view, "ask1", cx)
    });
    settle(cx);
    conversation
}

/// `/part` as the draft, sent: says whether anything was sent, and that
/// the reply is still gone.
fn send_part(
    conversation: &Entity<Conversation>,
    hub: &Entity<Hub>,
    cx: &mut VisualTestContext,
) -> bool {
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Submit("/part".into())));
    settle(cx);
    conversation.read_with(cx, |conversation, _| {
        assert!(
            conversation
                .draft
                .reply
                .as_ref()
                .is_some_and(|reply| reply.gone)
        );
    });
    hub.read_with(cx, |hub, _| hub.sent().len()) != before
}

#[gpui_kit::test]
fn a_reply_whose_thread_goes_is_kept_and_sent_nowhere_until_chosen(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    let conversation = gone_reply(&board, cx);
    assert!(!send_part(&conversation, &hub, cx), "nothing sent");
    // Escape keeps the guard: the next Enter still sends nothing.
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Cancel));
    settle(cx);
    assert!(!send_part(&conversation, &hub, cx), "nor after Escape");
    cx.update(|window, cx| window.click("irc-reply-mode-to-channel", cx));
    settle(cx);
    conversation.read_with(cx, |conversation, _| {
        assert_eq!(conversation.draft.reply, None)
    });
}

#[gpui_kit::test]
fn opening_other_threads_never_lifts_a_gone_reply_s_guard(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    let conversation = gone_reply(&board, cx);
    // Threads two and three are opened in turn.
    let room = crate::fixtures::channel("#aurora-lyra");
    for root in ["r1", "r2"] {
        board.update_in(cx, |board, window, cx| {
            crate::thread::open(board, &room, root.to_owned(), window, cx);
        });
        settle(cx);
    }
    assert!(!send_part(&conversation, &hub, cx), "nothing sent");
}

/// Opens thread `root` and chooses **Reply in thread** there.
fn reply_in(board: &Entity<Irc>, root: &str, cx: &mut VisualTestContext) {
    let room = crate::fixtures::channel("#aurora-lyra");
    board.update_in(cx, |board, window, cx| {
        crate::thread::open(board, &room, root.to_owned(), window, cx);
    });
    settle(cx);
    cx.update(|window, cx| window.click("irc-thread-reply", cx));
    settle(cx);
}

fn reply_root(conversation: &Entity<Conversation>, cx: &mut VisualTestContext) -> Option<String> {
    conversation.read_with(cx, |conversation, _| {
        conversation
            .draft
            .reply
            .as_ref()
            .map(|reply| reply.root.clone())
    })
}

#[gpui_kit::test]
fn reply_in_another_thread_keeps_a_gone_draft_guarded_until_moved(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    let conversation = gone_reply(&board, cx);
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text("Ready for review.", window, cx)
        });
    });
    reply_in(&board, "r1", cx);
    assert_eq!(
        reply_root(&conversation, cx).as_deref(),
        Some("ask1"),
        "the draft stays with its gone thread"
    );
    assert!(!send_part(&conversation, &hub, cx), "nothing sent");
    cx.update(|window, cx| window.click("irc-reply-mode-move", cx));
    settle(cx);
    conversation.read_with(cx, |conversation, _| {
        let reply = conversation.draft.reply.as_ref().unwrap();
        assert_eq!((reply.root.as_str(), reply.gone), ("r1", false), "moved");
    });
}

#[gpui_kit::test]
fn an_emptied_gone_draft_lets_another_thread_s_reply_start(cx: &mut TestAppContext) {
    let (board, _hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    let conversation = gone_reply(&board, cx);
    reply_in(&board, "r1", cx);
    assert_eq!(reply_root(&conversation, cx).as_deref(), Some("r1"));
}

#[gpui_kit::test]
fn a_thread_offered_to_a_gone_draft_that_goes_too_is_withdrawn(cx: &mut TestAppContext) {
    let (board, _hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    let conversation = gone_reply(&board, cx);
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text("Ready for review.", window, cx)
        });
    });
    reply_in(&board, "r1", cx);
    conversation.update(cx, |view, cx| crate::compose::reply::forget(view, "r1", cx));
    settle(cx);
    conversation.read_with(cx, |conversation, _| {
        let reply = conversation.draft.reply.as_ref().unwrap();
        assert_eq!((reply.root.as_str(), reply.gone), ("ask1", true));
        assert_eq!(reply.offered, None, "nothing to move to");
    });
}
