use subspace_client_irc::{Command, ConnectionStatus, Event};

use super::{CHANNEL_ONLY, COMMAND, GONE, command};
use crate::fixtures::{at, channel, network_id, query};
use crate::state::Network;
use crate::thread::{MESSAGE_TAGS, NO_REPLY_TAG, NOT_CONNECTED};
use crate::thread_status::tests::joined;

fn network(caps: &[&str]) -> Network {
    let mut state = joined(Vec::new());
    state.apply(
        Event::Capabilities {
            network: network_id(),
            enabled: caps.iter().map(|cap| (*cap).to_owned()).collect(),
        },
        at(1),
    );
    state.network(&network_id()).cloned().unwrap()
}

#[test]
fn replies_use_the_original_irc_id_and_allow_literal_slashes() {
    let room = channel("#aurora-lyra");
    let tagged = network(&[MESSAGE_TAGS]);
    assert_eq!(
        command(&room, "first", "//etc/hosts is fine", true, Some(&tagged)),
        Ok(Command::Reply {
            buffer: room,
            msgid: "first".into(),
            text: "/etc/hosts is fine".into()
        })
    );
}

#[test]
fn command_text_never_runs_from_a_reply() {
    let tagged = network(&[MESSAGE_TAGS]);
    for text in [" /msg mira hello", "/ready NAV-2", "/part"] {
        assert_eq!(
            command(&channel("#aurora-lyra"), "first", text, true, Some(&tagged)),
            Err(COMMAND)
        );
    }
}

#[test]
fn replies_wait_for_a_loaded_message_and_the_server_capability() {
    let room = channel("#aurora-lyra");
    let tagged = network(&[MESSAGE_TAGS]);
    assert_eq!(
        command(&room, "first", "Hello", false, Some(&tagged)),
        Err(GONE)
    );
    assert_eq!(
        command(&query("mira"), "first", "Hello", true, Some(&tagged)),
        Err(CHANNEL_ONLY)
    );
    assert_eq!(
        command(&room, "first", "Hello", true, Some(&network(&[]))),
        Err(NO_REPLY_TAG)
    );
    let mut down = tagged;
    down.status = ConnectionStatus::Disconnected;
    assert_eq!(
        command(&room, "first", "Hello", true, Some(&down)),
        Err(NOT_CONNECTED)
    );
}
