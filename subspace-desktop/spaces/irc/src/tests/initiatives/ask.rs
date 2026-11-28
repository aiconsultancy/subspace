//! The composer in a fleet channel: the command and mention actions, `/ask` offered
//! before the IRC client's commands, and `/ask` sent to the channel's agent
//! so the request status line tracks it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::Command;
use subspace_framework::dashboard::ChatInputEvent;

use super::super::remove::settle;
use super::fleet::ship;
use crate::conversation::Conversation;
use crate::fixtures::channel;
use crate::{Irc, hub::Hub};

const SHIP: &str = "#aurora-lyra";

fn front(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Entity<Conversation> {
    board.read_with(cx, |board, _| {
        board.dock.open[&channel(SHIP)].entity.clone()
    })
}

fn says(hub: &Entity<Hub>, text: &str, cx: &mut VisualTestContext) -> bool {
    hub.read_with(cx, |hub, _| {
        hub.sent().iter().any(
            |sent| matches!(sent, Command::Say { text: said, action: false, .. } if said == text),
        )
    })
}

#[gpui_kit::test]
fn the_channel_composer_opens_irc_commands_from_its_toolbar(cx: &mut TestAppContext) {
    let (board, _, cx) = ship(cx);
    cx.update(|window, cx| {
        assert!(window.find("irc-mention").visible());
        window.click("irc-commands", cx);
    });
    settle(cx);
    let conversation = front(&board, cx);
    conversation.read_with(cx, |view, cx| {
        assert_eq!(
            view.draft.offered(cx).first().map(String::as_str),
            Some("/ask")
        );
        assert!(view.composer().read(cx).text(cx).is_empty());
    });
}

#[gpui_kit::test]
fn ask_in_a_ship_channel_is_offered_and_sent_to_its_captain(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let conversation = front(&board, cx);
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    cx.update(|window, cx| input.update(cx, |input, cx| input.focus(window, cx)));
    cx.simulate_input("/as");
    settle(cx);
    let offered = conversation.read_with(cx, |conversation, cx| conversation.draft.offered(cx));
    assert_eq!(
        offered.first().map(String::as_str),
        Some("/ask"),
        "{offered:?}"
    );
    input.update(cx, |_, cx| {
        cx.emit(ChatInputEvent::Submit("/ask capture a baseline".into()))
    });
    settle(cx);
    assert!(says(&hub, "aurora-lyra: capture a baseline", cx));
    let kept = input.read_with(cx, |input, cx| input.text(cx));
    assert!(kept.is_empty(), "sent, so the draft clears: {kept:?}");
}
