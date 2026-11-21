use subspace_client_irc::{ConnectionStatus, Event, LineKind, Typing};

use super::{AttentionKind, IrcState, LINE_CAP, mentions, server_key};
use crate::fixtures::{at, channel, config, line, member, message, network_id, query};

mod attention;
mod backfill;
mod history;
mod listed;
mod removed;
mod statuses;

fn state() -> IrcState {
    IrcState::new(&[config()], vec!["Rocket".into()])
}

#[test]
fn configured_networks_start_with_a_server_buffer() {
    let state = state();
    assert_eq!(state.networks().len(), 1);
    assert_eq!(state.networks()[0].name, "Local Ergo");
    assert_eq!(state.networks()[0].nick, "subspace");
    assert!(state.buffer(&server_key(&network_id())).is_some());
    assert_eq!(state.connected_count(), 0);
}

#[test]
fn lines_count_unread_and_highlights_and_report_attention() {
    let mut state = state();
    let room = channel("#subspace");
    assert_eq!(
        state.apply(Event::Line(message(&room, "alice", "hello", 1)), at(1)),
        None
    );
    let mention = state.apply(
        Event::Line(message(&room, "bob", "rocket launch?", 2)),
        at(2),
    );
    assert_eq!(
        mention.map(|item| item.kind),
        Some(AttentionKind::Highlight)
    );
    assert_eq!(
        state.apply(Event::Line(message(&room, "subspace", "rocket", 3)), at(3)),
        None
    );
    let buffer = state.buffer(&room).unwrap();
    assert_eq!((buffer.unread, buffer.highlights), (2, 1));
    let private = state.apply(
        Event::Line(message(&query("carol"), "carol", "psst", 4)),
        at(4),
    );
    assert_eq!(private.map(|item| item.kind), Some(AttentionKind::Query));
    assert_eq!(
        state.attention_count(),
        2,
        "one mention plus one private message"
    );
}

#[test]
fn reading_moves_the_marker_and_clears_counts() {
    let mut state = state();
    let room = channel("#subspace");
    state.apply(Event::Line(message(&room, "alice", "one", 1)), at(1));
    assert_eq!(state.mark_read(&room), Some(at(1)));
    assert_eq!(state.mark_read(&room), None, "nothing new");
    state.apply(Event::Line(message(&room, "alice", "two", 2)), at(2));
    state.apply(Event::Line(message(&room, "alice", "three", 3)), at(3));
    state.apply(
        Event::ReadMarker {
            buffer: room.clone(),
            time: at(2),
        },
        at(3),
    );
    assert_eq!(
        state.buffer(&room).unwrap().unread,
        1,
        "another client read up to two"
    );
}

#[test]
fn membership_events_keep_the_member_list() {
    let mut state = state();
    let room = channel("#subspace");
    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(0),
    );
    state.apply(
        Event::Members {
            buffer: room.clone(),
            members: vec![member("alice", "@"), member("bob", "")],
        },
        at(0),
    );
    state.apply(
        Event::MemberChanged {
            buffer: room.clone(),
            old_nick: "bob".into(),
            member: member("bobby", "+"),
        },
        at(1),
    );
    // The runtime sends each member's stored spelling; others are other nicks.
    for nick in ["BOBBY", "alice"] {
        state.apply(
            Event::MemberLeft {
                buffer: room.clone(),
                nick: nick.into(),
            },
            at(2),
        );
    }
    let buffer = state.buffer(&room).unwrap();
    assert!(buffer.joined);
    assert_eq!(buffer.members, vec![member("bobby", "+")]);
    state.apply(
        Event::Left {
            buffer: room.clone(),
        },
        at(3),
    );
    assert!(!state.buffer(&room).unwrap().joined);
}

#[test]
fn failures_are_written_once_to_the_server_buffer() {
    let mut state = state();
    let failed = ConnectionStatus::Failed {
        error: "Connection refused".into(),
    };
    for minute in 0..2 {
        state.apply(
            Event::Status {
                network: network_id(),
                status: failed.clone(),
            },
            at(minute),
        );
    }
    let server = state.buffer(&server_key(&network_id())).unwrap();
    assert_eq!(server.entries.len(), 1);
    assert_eq!(server.entries[0].line.kind, LineKind::Error);
    assert_eq!(server.entries[0].line.text, "Connection refused");
    state.apply(
        Event::Status {
            network: network_id(),
            status: ConnectionStatus::Connected {
                nick: "subspace_".into(),
            },
        },
        at(3),
    );
    assert_eq!(state.nick(&network_id()), Some("subspace_"));
    assert_eq!(state.connected_count(), 1);
}

#[test]
fn typing_expires_and_ends_with_a_message() {
    let mut state = state();
    let room = channel("#subspace");
    let typing = |nick: &str, state| Event::Typing {
        buffer: room.clone(),
        nick: nick.into(),
        state,
    };
    state.apply(typing("alice", Typing::Active), at(0));
    state.apply(typing("bob", Typing::Paused), at(0));
    assert_eq!(state.buffer(&room).unwrap().typing.len(), 2);
    state.apply(Event::Line(message(&room, "bob", "done", 0)), at(0));
    assert!(state.expire_typing(at(1)));
    assert!(state.buffer(&room).unwrap().typing.is_empty());
    state.apply(typing("alice", Typing::Active), at(2));
    state.apply(typing("alice", Typing::Done), at(2));
    assert!(state.buffer(&room).unwrap().typing.is_empty());
}

#[test]
fn live_lines_are_capped_from_the_oldest() {
    let mut state = state();
    let room = channel("#busy");
    for index in 0..(LINE_CAP as i64 + 5) {
        state.apply(
            Event::Line(line(&room, LineKind::Join, "x", "", index)),
            at(index),
        );
    }
    let buffer = state.buffer(&room).unwrap();
    assert_eq!(buffer.entries.len(), LINE_CAP);
    assert_eq!(buffer.entries[0].line.time, at(5));
}

#[test]
fn highlight_words_match_whole_words_only() {
    let words = ["rocket".to_owned()];
    assert!(mentions("Rocket!", &words));
    assert!(mentions("the \u{2}rocket\u{2} is ready", &words));
    assert!(!mentions("rockets", &words));
    assert!(!mentions("skyrocket", &words));
}

#[test]
fn only_messages_highlight() {
    let mut state = state();
    let room = channel("#subspace");
    let topic = line(
        &room,
        LineKind::Topic,
        "alice",
        "changed the topic to: rocket",
        0,
    );
    assert_eq!(state.apply(Event::Line(topic), at(0)), None);
    let notice = line(&room, LineKind::Notice, "bob", "rocket", 1);
    assert!(state.apply(Event::Line(notice), at(1)).is_some());
    assert_eq!(state.buffer(&room).unwrap().highlights, 1);
}
