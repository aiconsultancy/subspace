//! Direct messages are for people only: every way Channels starts a private
//! conversation refuses a fleet agent, shows where it talks and offers to go
//! there, while a person's private conversation opens as before and an old
//! one with an agent stays readable.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Command, Event, Target};
use subspace_framework::dashboard::ChatInputEvent;

use super::super::inject;
use super::super::remove::settle;
use super::fleet::ship;
use crate::conversation::{Conversation, ConversationEvent};
use crate::fixtures::{channel, message, network_id, query};
use crate::hub::Hub;
use crate::markup::Ref;
use crate::{Irc, view};

const SHIP: &str = "#aurora-lyra";
const NOTICE: &str = "aurora-lyra is a fleet agent. Agents talk in channels, where everyone can see: ask in #aurora-lyra.";

fn front(
    board: &Entity<Irc>,
    buffer: &BufferKey,
    cx: &mut VisualTestContext,
) -> Entity<Conversation> {
    board.read_with(cx, |board, _| {
        board
            .dock
            .open
            .get(buffer)
            .map(|hosted| hosted.entity.clone())
            .expect("the conversation is open")
    })
}

fn notice(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, _| {
        window
            .try_find("irc-agent-notice")
            .and_then(|found| found.label().map(str::to_owned))
    })
}

/// No private conversation with `nick` was asked for or held.
fn no_query(hub: &Entity<Hub>, nick: &str, cx: &mut VisualTestContext) {
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent()
                .iter()
                .any(|sent| matches!(sent, Command::Query { nick: asked, .. } if asked == nick)),
            "no query opened with {nick}"
        );
        assert!(hub.state.buffer(&query(nick)).is_none());
    });
}

fn submit(conversation: &Entity<Conversation>, text: &str, cx: &mut VisualTestContext) {
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    let text = text.to_owned();
    input.update(cx, |_, cx| cx.emit(ChatInputEvent::Submit(text)));
    settle(cx);
}

fn sent_input(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> bool {
    hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .any(|sent| matches!(sent, Command::Input { .. }))
    })
}

#[gpui_kit::test]
fn choosing_an_agent_in_the_member_list_shows_where_it_talks(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    assert_eq!(
        cx.update(|window, _| window
            .find("irc-member:aurora-lyra")
            .label()
            .map(str::to_owned))
            .as_deref(),
        Some("Show where to talk with aurora-lyra")
    );
    cx.update(|window, cx| window.click("irc-member:aurora-lyra", cx));
    settle(cx);
    no_query(&hub, "aurora-lyra", cx);
    assert_eq!(notice(cx).as_deref(), Some(NOTICE));
    // A person still opens a private conversation.
    cx.update(|window, cx| window.click("irc-member:mira", cx));
    settle(cx);
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Query {
            network: network_id(),
            nick: "mira".into(),
        }));
    });
}

#[gpui_kit::test]
fn a_nick_reference_to_an_agent_opens_no_private_conversation(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let conversation = front(&board, &channel(SHIP), cx);
    conversation.update(cx, |_, cx| {
        cx.emit(ConversationEvent::Open(Ref::Nick("aurora-admiral".into())))
    });
    settle(cx);
    no_query(&hub, "aurora-admiral", cx);
    assert_eq!(
        notice(cx).as_deref(),
        Some(
            "aurora-admiral is a fleet agent. Agents talk in channels, where everyone can see: ask in #aurora-starbase."
        )
    );
}

#[gpui_kit::test]
fn msg_and_query_to_an_agent_keep_the_draft_and_show_where_it_talks(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let conversation = front(&board, &channel(SHIP), cx);
    for text in ["/msg aurora-lyra are you free?", "/query Aurora-Lyra"] {
        submit(&conversation, text, cx);
        assert!(!sent_input(&hub, cx), "{text} is not sent");
        assert!(notice(cx).is_some_and(|said| said.contains("ask in #aurora-lyra")));
        cx.update(|window, cx| window.click("irc-agent-notice-dismiss", cx));
        settle(cx);
        assert_eq!(notice(cx), None, "dismissed");
    }
    submit(&conversation, "/msg mira hello", &mut *cx);
    assert!(sent_input(&hub, cx), "a person's private message is sent");
}

#[gpui_kit::test]
fn go_to_the_agent_s_channel_from_the_notice(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let starbase = channel("#aurora-starbase");
    inject(
        &hub,
        vec![Event::Joined {
            buffer: starbase.clone(),
        }],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &starbase, cx));
    settle(cx);
    let conversation = front(&board, &starbase, cx);
    submit(&conversation, "/query aurora-lyra", cx);
    cx.update(|window, cx| window.click("irc-agent-notice-go", cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&channel(SHIP)));
    });
    no_query(&hub, "aurora-lyra", cx);
}

#[gpui_kit::test]
fn an_old_private_conversation_with_an_agent_stays_readable_but_sends_nothing(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = ship(cx);
    let old = query("aurora-lyra");
    inject(
        &hub,
        vec![Event::Line(super::fleet::signed(message(
            &old,
            "aurora-lyra",
            "From before the rule.",
            9,
        )))],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &old, cx));
    settle(cx);
    let conversation = front(&board, &old, cx);
    let seq = hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&old)
            .and_then(|held| held.entries.front())
            .map(|entry| entry.seq)
            .unwrap_or_default()
    });
    cx.update(|window, _| {
        assert!(
            window.find(format!("irc-line:{seq}:row")).visible(),
            "readable"
        );
    });
    submit(&conversation, "still there?", cx);
    assert!(!sent_input(&hub, cx));
    assert_eq!(notice(cx).as_deref(), Some(NOTICE));
    assert!(matches!(old.target, Target::Query(_)));
}
