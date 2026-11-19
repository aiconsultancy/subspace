//! The real IRC client answering the tray's recorded sends, as connected
//! in different ways.
use crate::conversation::Conversation;
use gpui_kit::{Entity, VisualTestContext};
use subspace_client_irc::Command;
use subspace_client_irc::test_support::Rehearsal;

/// How the client answering a recorded send is connected.
pub(super) enum Rehearsed<'a> {
    /// Welcomed on a server offering these caps; the write succeeds.
    Welcomed(&'a str),
    /// Welcomed, but the connection fails while writing.
    LostWhileWriting(&'a str),
    /// Agreed these caps, not yet welcomed.
    Registering(&'a str),
}

/// Answers the newest recorded `SendAll` as the real client does
/// when connected as `how`, and hands its events to the hub. Returns what
/// went to the server.
pub(super) fn rehearse(
    view: &Entity<Conversation>,
    how: Rehearsed<'_>,
    cx: &mut VisualTestContext,
) -> Vec<String> {
    let (hub, room) = view.read_with(cx, |view, _| (view.hub.clone(), view.buffer.clone()));
    let command = hub.read_with(cx, |hub, _| {
        hub.sent()
            .iter()
            .rev()
            .find(|command| matches!(command, Command::SendAll { .. }))
            .cloned()
    });
    let command = command.expect("a recorded send");
    let subspace_client_irc::Target::Channel(name) = &room.target else {
        panic!("a channel conversation");
    };
    let (config, at) = (crate::fixtures::config(), crate::fixtures::at(4));
    let (mut client, lost) = match how {
        Rehearsed::Welcomed(caps) => (Rehearsal::new(config, caps, name, at), false),
        Rehearsed::LostWhileWriting(caps) => (Rehearsal::new(config, caps, name, at), true),
        Rehearsed::Registering(caps) => (Rehearsal::registering(config, caps, at), false),
    };
    let answer = client.command(command);
    let events = match lost {
        true => answer.lost,
        false => answer.events,
    };
    crate::tests::inject(&hub, events, cx);
    cx.run_until_parked();
    answer.wire
}
