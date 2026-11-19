//! The server's channel list in the state, and joining from it.

use subspace_client_irc::{Command, Event, ListedChannel, NetworkId};

use super::state;
use crate::fixtures::{at, channel, network_id};

fn list(network: NetworkId, names: &[&str]) -> Event {
    Event::ChannelList {
        network,
        channels: names
            .iter()
            .map(|name| ListedChannel {
                name: (*name).into(),
                users: 1,
                topic: String::new(),
            })
            .collect(),
    }
}

#[test]
fn a_new_list_replaces_the_last_and_marks_it_for_the_cache() {
    let mut state = state();
    state.apply(list(network_id(), &["#old"]), at(1));
    state.apply(list(network_id(), &["#new"]), at(2));
    let directory = state.directories.get(&network_id());
    assert_eq!(directory.map(|known| known.fetched), Some(at(2)));
    assert_eq!(
        directory.map(|known| known.channels[0].name.as_str()),
        Some("#new")
    );
    assert!(state.directories.take_changed());

    state.apply(list(NetworkId("elsewhere".into()), &["#x"]), at(3));
    assert!(
        state
            .directories
            .get(&NetworkId("elsewhere".into()))
            .is_none(),
        "a network that is not configured keeps no list"
    );
    assert!(!state.directories.take_changed());
}

#[test]
fn a_channel_joined_from_the_list_opens_once_the_server_joins_us() {
    let mut state = state();
    let room = channel("#rust");
    assert_eq!(
        state.join_listed(&room),
        Command::Join {
            network: network_id(),
            channel: "#rust".into(),
            key: None,
        }
    );
    assert_eq!(state.take_opened(), None, "nothing opens before the join");
    state.apply(
        Event::Joined {
            buffer: channel("#Rust"),
        },
        at(1),
    );
    assert_eq!(state.take_opened(), Some(channel("#Rust")));
    assert!(
        state
            .buffer(&channel("#Rust"))
            .is_some_and(|held| held.joined)
    );

    state.apply(
        Event::Joined {
            buffer: channel("#other"),
        },
        at(2),
    );
    assert_eq!(state.take_opened(), None, "other joins open nothing");
}
