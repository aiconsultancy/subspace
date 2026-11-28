//! Nick-wide status lines: held per network, cleaned again, cleared by the
//! runtime, by departures and by the connection ending, and never ended by
//! the nick's own messages.

use subspace_client_irc::{ConnectionStatus, Event, Typing};

use super::state;
use crate::fixtures::{at, channel, member, message, network_id};
use crate::state::IrcState;

fn status(nick: &str, text: Option<&str>) -> Event {
    Event::NickStatus {
        network: network_id(),
        nick: nick.into(),
        status: text.map(str::to_owned),
    }
}

fn held(state: &IrcState, nick: &str) -> Option<String> {
    state
        .network(&network_id())
        .and_then(|network| network.statuses.get(nick))
        .map(str::to_owned)
}

fn connection(status: ConnectionStatus) -> Event {
    Event::Status {
        network: network_id(),
        status,
    }
}

fn connected() -> Event {
    connection(ConnectionStatus::Connected {
        nick: "subspace".into(),
    })
}

/// Connected, in `#subspace` and `#ops` with galactica, and galactica also
/// alone with us in `#galactica`.
fn joined() -> IrcState {
    let mut state = state();
    state.apply(connected(), at(0));
    for (room, members) in [
        (
            "#subspace",
            vec![member("galactica", ""), member("alice", "")],
        ),
        ("#ops", vec![member("Galactica", "@")]),
    ] {
        state.apply(
            Event::Members {
                buffer: channel(room),
                members,
            },
            at(0),
        );
    }
    state
}

#[test]
fn a_status_is_set_replaced_and_cleared() {
    let mut state = joined();
    state.apply(status("galactica", Some("Galactica is thinking…")), at(1));
    assert_eq!(
        held(&state, "galactica").as_deref(),
        Some("Galactica is thinking…")
    );
    assert_eq!(
        held(&state, "GALACTICA").as_deref(),
        Some("Galactica is thinking…"),
        "nicks compare ignoring ASCII case"
    );
    state.apply(status("Galactica", Some("Galactica is reading…")), at(2));
    assert_eq!(
        held(&state, "galactica").as_deref(),
        Some("Galactica is reading…"),
        "another spelling replaces the status rather than adding one"
    );
    let statuses = &state.network(&network_id()).unwrap().statuses;
    assert_eq!(statuses.iter().count(), 1);
    state.apply(status("galactica", None), at(3));
    assert_eq!(held(&state, "galactica"), None);
    state.apply(status("galactica", Some("Galactica is thinking…")), at(4));
    state.apply(status("galactica", Some(" \u{2}\u{3}04 \r\n ")), at(5));
    assert_eq!(
        held(&state, "galactica"),
        None,
        "nothing left after cleaning is a clear"
    );
}

#[test]
fn statuses_are_per_network_not_per_buffer() {
    let mut state = joined();
    state.apply(status("galactica", Some("Galactica is thinking…")), at(1));
    for room in ["#subspace", "#ops"] {
        let buffer = state.buffer(&channel(room)).unwrap();
        assert!(buffer.typing.is_empty(), "{room} gains no typing notice");
        assert!(buffer.entries.is_empty(), "{room} gains no line");
    }
    assert_eq!(state.buffer(&channel("#subspace")).unwrap().unread, 0);
}

#[test]
fn an_over_long_or_multi_line_value_is_cleaned_where_it_is_held() {
    let mut state = joined();
    let long = "a".repeat(200);
    state.apply(status("galactica", Some(&long)), at(1));
    let shown = held(&state, "galactica").unwrap();
    assert_eq!(shown.chars().count(), 80, "clamped to 80 characters");
    assert!(shown.ends_with('…'), "the cut is marked");
    state.apply(
        status(
            "galactica",
            Some("Galactica is thinking…\nabout\r\nthe plan"),
        ),
        at(2),
    );
    assert_eq!(
        held(&state, "galactica").as_deref(),
        Some("Galactica is thinking… about  the plan"),
        "one line"
    );
}

#[test]
fn the_nick_s_own_messages_end_its_typing_but_not_its_status() {
    let mut state = joined();
    let room = channel("#subspace");
    state.apply(status("galactica", Some("Galactica is thinking…")), at(1));
    state.apply(
        Event::Typing {
            buffer: room.clone(),
            nick: "galactica".into(),
            state: Typing::Active,
        },
        at(1),
    );
    state.apply(Event::Line(message(&room, "galactica", "Done.", 1)), at(1));
    assert!(state.buffer(&room).unwrap().typing.is_empty());
    assert_eq!(
        held(&state, "galactica").as_deref(),
        Some("Galactica is thinking…"),
        "the gateway clears the status itself"
    );
}

#[test]
fn quitting_clears_the_status_through_the_runtime_s_clear() {
    let mut state = joined();
    state.apply(status("galactica", Some("Galactica is thinking…")), at(1));
    // The runtime clears a status when its nick quits.
    state.apply(status("galactica", None), at(2));
    assert_eq!(held(&state, "galactica"), None);
}

#[test]
fn leaving_every_channel_we_hold_clears_the_status() {
    let mut state = joined();
    state.apply(status("galactica", Some("Galactica is thinking…")), at(1));
    state.apply(
        Event::MemberLeft {
            buffer: channel("#subspace"),
            nick: "galactica".into(),
        },
        at(2),
    );
    assert_eq!(
        held(&state, "galactica").as_deref(),
        Some("Galactica is thinking…"),
        "still in #ops, under another spelling"
    );
    state.apply(
        Event::MemberLeft {
            buffer: channel("#ops"),
            nick: "Galactica".into(),
        },
        at(3),
    );
    assert_eq!(
        held(&state, "galactica"),
        None,
        "no channel we hold lists it any more, even if the runtime's clear is late"
    );
}

#[test]
fn a_dropped_connection_leaves_nothing_stale_after_reconnecting() {
    let mut state = joined();
    state.apply(status("galactica", Some("Galactica is thinking…")), at(1));
    state.apply(
        status("discovery", Some("Discovery is noodling on it…")),
        at(1),
    );
    state.apply(
        connection(ConnectionStatus::Reconnecting {
            attempt: 1,
            delay_secs: 2,
            error: "Connection reset".into(),
        }),
        at(2),
    );
    assert_eq!(
        state
            .network(&network_id())
            .unwrap()
            .statuses
            .iter()
            .count(),
        0,
        "dropped without waiting for the runtime's clears"
    );
    state.apply(connection(ConnectionStatus::Connecting), at(3));
    state.apply(connection(ConnectionStatus::Registering), at(3));
    state.apply(connected(), at(4));
    assert_eq!(held(&state, "galactica"), None);
    assert_eq!(held(&state, "discovery"), None);
    state.apply(status("galactica", Some("Galactica is reading…")), at(5));
    assert_eq!(
        held(&state, "galactica").as_deref(),
        Some("Galactica is reading…"),
        "a status set after reconnecting shows"
    );
}

#[test]
fn reconnecting_waits_for_each_channels_new_join() {
    let mut state = crate::state::IrcState::new(&[crate::fixtures::config()], Vec::new());
    let room = crate::fixtures::channel("#example-ship");
    let set_status = |state: &mut crate::state::IrcState, status| {
        state.apply(
            subspace_client_irc::Event::Status {
                network: room.network.clone(),
                status,
            },
            crate::fixtures::at(0),
        );
    };
    let connected = subspace_client_irc::ConnectionStatus::Connected {
        nick: "subspace".into(),
    };
    set_status(&mut state, connected.clone());
    let joined = subspace_client_irc::Event::Joined {
        buffer: room.clone(),
    };
    state.apply(joined.clone(), crate::fixtures::at(0));
    assert!(state.buffer(&room).unwrap().joined);

    set_status(
        &mut state,
        subspace_client_irc::ConnectionStatus::Disconnected,
    );
    set_status(&mut state, connected);
    assert!(!state.buffer(&room).unwrap().joined);

    state.apply(joined, crate::fixtures::at(0));
    assert!(state.buffer(&room).unwrap().joined);
}
