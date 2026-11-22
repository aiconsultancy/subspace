use subspace_client_irc::{ConnectionStatus, Event, Target};
use subspace_framework::dashboard::Tone;

use super::{Layout, Rosters, accessible_label, row_key, status_label};
use crate::fixtures::{at, channel, config, message, network_id, query};
use crate::state::{IrcState, server_key};

/// The layout of a state whose networks serve no fleet.
fn layout(state: &IrcState) -> Layout {
    super::layout(state, &Rosters::default())
}

fn titles(state: &IrcState) -> Vec<String> {
    layout(state).groups[0]
        .rows
        .iter()
        .map(|row| row.title.to_string())
        .collect()
}

#[test]
fn server_then_channels_then_queries_each_sorted() {
    let mut state = IrcState::new(&[config()], Vec::new());
    for buffer in [
        channel("#zeta"),
        query("bob"),
        channel("#Alpha"),
        query("Anna"),
    ] {
        state.apply(Event::Joined { buffer }, at(0));
    }
    assert_eq!(titles(&state), ["Server", "#Alpha", "#zeta", "Anna", "bob"]);
    let layout = layout(&state);
    assert_eq!(layout.groups[0].name.as_ref(), "Local Ergo");
    assert_eq!(
        layout.buffers.get(&row_key(&server_key(&network_id()))),
        Some(&server_key(&network_id()))
    );
}

#[test]
fn rows_carry_counts_and_parted_channels_are_muted() {
    let mut state = IrcState::new(&[config()], vec!["rocket".into()]);
    let room = channel("#subspace");
    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(0),
    );
    state.apply(Event::Line(message(&room, "alice", "hi", 1)), at(1));
    let rows = layout(&state).groups.remove(0).rows;
    let row = &rows[1];
    assert_eq!(row.trailing[0].text.as_ref(), "1");
    assert_eq!(row.trailing[0].tone, Tone::Muted);
    assert!(!row.done);
    state.apply(Event::Line(message(&room, "bob", "rocket", 2)), at(2));
    state.apply(
        Event::Left {
            buffer: room.clone(),
        },
        at(3),
    );
    let rows = layout(&state).groups.remove(0).rows;
    assert_eq!(
        rows[1].trailing[0].text.as_ref(),
        "@",
        "a mention's own mark"
    );
    assert_eq!(rows[1].trailing[0].tone, Tone::Red);
    assert_eq!(rows[1].trailing[1].tone, Tone::Accent);
    assert!(rows[1].done);
    let buffer = state.buffer(&room).unwrap();
    assert_eq!(
        accessible_label("#subspace", buffer).as_ref(),
        "#subspace, 2 unread, 1 mentioning you"
    );
}

#[test]
fn group_headers_name_the_connection_state_in_text_and_tone() {
    let mut state = IrcState::new(&[config()], Vec::new());
    let header = |state: &IrcState| {
        let group = layout(state).groups.remove(0);
        (group.count.map(|count| count.to_string()), group.icon.tone)
    };
    assert_eq!(header(&state), (Some("Disconnected".into()), Tone::Muted));
    for (status, label, tone) in [
        (ConnectionStatus::Connecting, "Connecting…", Tone::Yellow),
        (
            ConnectionStatus::Connected { nick: "me".into() },
            "Connected",
            Tone::Green,
        ),
        (
            ConnectionStatus::Failed {
                error: "refused".into(),
            },
            "Failed",
            Tone::Red,
        ),
    ] {
        assert_eq!(status_label(&status), label);
        state.apply(
            Event::Status {
                network: network_id(),
                status,
            },
            at(0),
        );
        assert_eq!(header(&state), (Some(label.into()), tone));
    }
}

#[test]
fn row_keys_are_distinct_per_kind() {
    let channel_key = row_key(&channel("#a"));
    let query_key = row_key(&query("#a"));
    assert_ne!(channel_key, query_key);
    assert_ne!(row_key(&server_key(&network_id())), channel_key);
    assert!(matches!(channel("#a").target, Target::Channel(_)));
}

fn listed(name: &str, users: u32) -> subspace_client_irc::ListedChannel {
    subspace_client_irc::ListedChannel {
        name: name.into(),
        users,
        topic: String::new(),
    }
}

fn channel_list(names: &[(&str, u32)]) -> Event {
    Event::ChannelList {
        network: network_id(),
        channels: names
            .iter()
            .map(|(name, users)| listed(name, *users))
            .collect(),
    }
}

#[test]
fn listed_channels_we_are_not_in_follow_the_buffers_greyed_by_name() {
    let mut state = IrcState::new(&[config()], Vec::new());
    assert_eq!(
        titles(&state),
        ["Server"],
        "no list yet shows nothing extra"
    );
    for buffer in [channel("#subspace"), query("anna")] {
        state.apply(Event::Joined { buffer }, at(0));
    }
    state.apply(
        channel_list(&[("#zulu", 2), ("#Subspace", 4), ("#alpha", 1)]),
        at(1),
    );
    assert_eq!(
        titles(&state),
        ["Server", "#subspace", "anna", "#alpha", "#zulu"],
        "a listed channel we are in shows once, as its buffer"
    );
    let layout = layout(&state);
    let rows = &layout.groups[0].rows;
    let alpha = &rows[3];
    assert!(alpha.done, "greyed");
    assert_eq!(alpha.trailing.len(), 1, "a user count, no unread count");
    assert_eq!(alpha.trailing[0].text.as_ref(), "1 user");
    assert_eq!(rows[4].trailing[0].text.as_ref(), "2 users");
    assert_eq!(
        layout.listed.get(&alpha.key),
        Some(&channel("#alpha")),
        "the row names the channel to join"
    );
    assert!(!layout.buffers.contains_key(&alpha.key));
    assert_ne!(alpha.key, row_key(&channel("#alpha")));
}

#[test]
fn a_parted_channel_stays_its_greyed_buffer_and_a_removed_one_returns_to_the_list() {
    let mut state = IrcState::new(&[config()], Vec::new());
    let room = channel("#subspace");
    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(0),
    );
    state.apply(channel_list(&[("#subspace", 3)]), at(1));
    state.apply(
        Event::Left {
            buffer: room.clone(),
        },
        at(2),
    );
    let rows = layout(&state).groups.remove(0).rows;
    assert_eq!(rows.len(), 2, "no second row for the parted channel");
    assert!(rows[1].done, "greyed");
    state.remove(&room);
    let layout = layout(&state);
    assert_eq!(titles(&state), ["Server", "#subspace"]);
    assert!(layout.listed.values().any(|key| key == &room));
}

#[test]
fn search_finds_listed_channels_as_not_joined() {
    let mut state = IrcState::new(&[config()], Vec::new());
    state.apply(channel_list(&[("#rust", 9)]), at(1));
    let layout = layout(&state);
    let index = super::search(&layout);
    let found = index.search("rust", 5);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.title.as_ref(), "#rust");
}

mod fleets;
