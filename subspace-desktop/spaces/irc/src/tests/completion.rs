//! Completion in the real composer: typed keys open the list above the
//! input, and Enter or Tab puts the highlighted choice in the draft.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{ConnectionStatus, Event};
use subspace_framework::dashboard::ChatInput;

use super::{inject, mount, settings};
use crate::conversation::Conversation;
use crate::fixtures::{channel, member, message, network_id};
use crate::view;

fn crew_events() -> Vec<Event> {
    let room = channel("#ship-galactica");
    vec![
        Event::Status {
            network: network_id(),
            status: ConnectionStatus::Connected {
                nick: "subspace".into(),
            },
        },
        Event::Joined {
            buffer: room.clone(),
        },
        Event::Members {
            buffer: room.clone(),
            members: vec![
                member("galactica-captain", "@"),
                member("galactica-pilot", ""),
                member("gaius", "+"),
                member("subspace", ""),
            ],
        },
        Event::Line(message(&room, "galactica-captain", "report", 1)),
    ]
}

/// The open `#ship-galactica` conversation and its composer, focused.
pub(super) fn composer(
    cx: &mut TestAppContext,
) -> (
    Entity<Conversation>,
    Entity<ChatInput>,
    &mut VisualTestContext,
) {
    let (board, hub, cx) = mount(cx, settings());
    // After Kit's own bindings, as the shell does, so the list's keys win.
    cx.update(|_, cx| subspace_framework::dashboard::chat_input::bind_keys(cx));
    inject(&hub, crew_events(), cx);
    let conversation = reveal(&board, &channel("#ship-galactica"), cx);
    let input = focused_composer(&conversation, cx);
    (conversation, input, cx)
}

/// Shows `room` on `board` and returns its conversation.
pub(super) fn reveal(
    board: &Entity<crate::Irc>,
    room: &subspace_client_irc::BufferKey,
    cx: &mut VisualTestContext,
) -> Entity<Conversation> {
    board.update(cx, |board, cx| view::reveal(board, room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    board.read_with(cx, |board, _| board.dock.open[room].entity.clone())
}

/// `conversation`'s composer, focused.
pub(super) fn focused_composer(
    conversation: &Entity<Conversation>,
    cx: &mut VisualTestContext,
) -> Entity<ChatInput> {
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.focus(window, cx));
        window.render_frame(cx);
    });
    input
}

/// The labels the list shows now.
pub(super) fn labels(
    conversation: &Entity<Conversation>,
    cx: &mut VisualTestContext,
) -> Vec<String> {
    cx.update(|window, cx| window.render_frame(cx));
    conversation.read_with(cx, |conversation, cx| conversation.draft.offered(cx))
}

pub(super) fn text(input: &Entity<ChatInput>, cx: &mut VisualTestContext) -> String {
    input.read_with(cx, |input, cx| input.text(cx))
}

#[gpui_kit::test]
fn typing_at_gal_offers_the_galactica_crew_and_enter_inserts_the_mention(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("@gal");
    assert_eq!(
        labels(&conversation, cx),
        ["galactica-captain", "galactica-pilot"],
        "the recent speaker first"
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&input, cx), "@galactica-captain ");
    assert!(
        labels(&conversation, cx).is_empty(),
        "choosing closes the list"
    );
}

#[gpui_kit::test]
fn down_then_tab_chooses_the_next_member_mid_message(cx: &mut TestAppContext) {
    let (_, input, cx) = composer(cx);
    cx.simulate_input("ask @gal");
    cx.simulate_keystrokes("down tab");
    assert_eq!(text(&input, cx), "ask @galactica-pilot ");
}

#[gpui_kit::test]
fn escape_closes_the_list_and_keeps_the_draft(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("@ga");
    assert_eq!(labels(&conversation, cx).len(), 3);
    cx.simulate_keystrokes("escape");
    assert!(labels(&conversation, cx).is_empty());
    assert_eq!(text(&input, cx), "@ga", "the draft stays");
}

#[gpui_kit::test]
fn a_slash_offers_commands_and_then_their_arguments(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("/jo");
    assert_eq!(labels(&conversation, cx), ["/join"]);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&input, cx), "/join ");
    assert_eq!(
        labels(&conversation, cx),
        ["#ship-galactica"],
        "the channel argument follows"
    );
}

#[gpui_kit::test]
fn tab_without_a_list_completes_a_bare_nick(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.simulate_input("gai");
    assert!(
        labels(&conversation, cx).is_empty(),
        "a bare word opens no list"
    );
    cx.simulate_keystrokes("tab");
    assert_eq!(text(&input, cx), "gaius: ");
}
