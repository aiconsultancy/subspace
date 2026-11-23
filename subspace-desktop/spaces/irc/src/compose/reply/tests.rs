//! What reply mode sends, to where and with which tag, and when it refuses.

use subspace_client_irc::{Command, ConnectionStatus, Event};

use super::{ATTACHMENTS, CHANNEL_ONLY, COMMAND, GONE, ReplyTo, command, placeholder};
use crate::fixtures::{at, channel, network_id, query};
use crate::state::{IrcState, Network, server_key};
use crate::thread::{MESSAGE_TAGS, NO_REPLY_TAG, NOT_CONNECTED};
use crate::thread_status::tests::joined;

/// The fixture network, connected, having agreed `caps`.
fn network(caps: &[&str]) -> Network {
    let mut state: IrcState = joined(Vec::new());
    state.apply(
        Event::Capabilities {
            network: network_id(),
            enabled: caps.iter().map(|cap| (*cap).to_owned()).collect(),
        },
        at(1),
    );
    state.network(&network_id()).cloned().unwrap()
}

fn root() -> ReplyTo {
    ReplyTo::new(
        "first",
        "mira",
        "Morning, Lyra. The pilot review is on \u{2}Thursday\u{2}.",
    )
}

#[test]
fn a_reply_goes_to_the_channel_with_the_root_s_reply_tag() {
    let room = channel("#aurora-lyra");
    let tagged = network(&[MESSAGE_TAGS]);
    assert_eq!(
        command(&room, &root(), "On it.", Some(&tagged)),
        Ok(Command::Reply {
            buffer: room.clone(),
            msgid: "first".into(),
            text: "On it.".into(),
        }),
        "to the channel, never a nick"
    );
    assert_eq!(
        command(&room, &root(), "//etc/hosts is fine", Some(&tagged)),
        Ok(Command::Reply {
            buffer: room.clone(),
            msgid: "first".into(),
            text: "/etc/hosts is fine".into(),
        }),
        "a doubled slash is a literal one"
    );
    assert_eq!(
        command(&room, &root(), "/msg aurora-lyra hi", Some(&tagged)),
        Err(COMMAND),
        "a command is never sent as a reply"
    );
}

#[test]
fn a_reply_is_refused_without_the_reply_tag_a_connection_or_a_channel() {
    let room = channel("#aurora-lyra");
    assert_eq!(
        command(&room, &root(), "On it.", Some(&network(&[]))),
        Err(NO_REPLY_TAG)
    );
    let mut down = network(&[MESSAGE_TAGS]);
    down.status = ConnectionStatus::Disconnected;
    assert_eq!(
        command(&room, &root(), "On it.", Some(&down)),
        Err(NOT_CONNECTED)
    );
    assert_eq!(command(&room, &root(), "On it.", None), Err(NOT_CONNECTED));
    let tagged = network(&[MESSAGE_TAGS]);
    for elsewhere in [query("aurora-lyra"), server_key(&network_id())] {
        assert_eq!(
            command(&elsewhere, &root(), "On it.", Some(&tagged)),
            Err(CHANNEL_ONLY),
            "never a private message"
        );
    }
    assert!(ATTACHMENTS.contains("Cancel the reply"));
}

#[test]
fn the_reply_row_names_the_author_and_the_start_of_the_root() {
    let reply = root();
    assert_eq!(reply.root, "first");
    assert_eq!(
        reply.words(),
        "Replying in thread to mira: Morning, Lyra. The pilot review is on Thursday."
    );
    assert_eq!(placeholder(&reply), "Reply in thread to mira");
    let long = ReplyTo::new("long", "tavi", &"word ".repeat(40));
    assert_eq!(long.excerpt.chars().count(), 80);
    assert!(long.excerpt.ends_with('…'));
}

#[test]
fn a_reply_whose_thread_is_gone_is_never_sent() {
    let room = channel("#aurora-lyra");
    let tagged = network(&[MESSAGE_TAGS]);
    let gone = ReplyTo {
        gone: true,
        ..root()
    };
    assert_eq!(command(&room, &gone, "On it.", Some(&tagged)), Err(GONE));
    assert_eq!(
        command(&room, &gone, "/part", Some(&tagged)),
        Err(GONE),
        "nor run"
    );
}
