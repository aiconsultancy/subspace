//! Lines that merge among the held ones: what was missed since the newest
//! stored line, the lines around one message, and lines restored from the
//! store.

use subspace_client_irc::{Event, Line, Target};

use super::state;
use crate::fixtures::{at, channel, message, network_id, query};
use crate::state::IrcState;

fn with_id(mut line: Line, id: &str) -> Line {
    line.id = Some(id.into());
    line
}

fn texts(state: &IrcState, buffer: &subspace_client_irc::BufferKey) -> Vec<String> {
    state
        .buffer(buffer)
        .map(|held| {
            held.entries
                .iter()
                .map(|entry| entry.line.text.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn missed_lines_go_in_time_order_once_each() {
    let mut state = state();
    let room = channel("#subspace");
    state.apply(
        Event::Line(with_id(message(&room, "alice", "one", 1), "a")),
        at(1),
    );
    state.apply(Event::Line(message(&room, "bob", "four", 4)), at(4));
    let missed = vec![
        with_id(message(&room, "alice", "one", 1), "a"),
        message(&room, "carol", "two", 2),
        message(&room, "carol", "two", 2),
        with_id(message(&room, "dave", "three", 3), "c"),
        // The held line without a msgid, sent again with one.
        with_id(message(&room, "bob", "four", 4), "d"),
    ];
    state.apply(
        Event::Backfill {
            buffer: room.clone(),
            lines: missed,
        },
        at(5),
    );
    assert_eq!(texts(&state, &room), ["one", "two", "three", "four"]);
    let held = state.buffer(&room).unwrap();
    let mut seqs: Vec<i64> = held.entries.iter().map(|entry| entry.seq).collect();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(seqs.len(), 4, "every line keeps its own identity");
}

#[test]
fn restored_lines_count_nothing_and_skip_removed_buffers() {
    let mut state = state();
    let room = channel("#subspace");
    let carol = query("carol");
    state.restore(
        &room,
        vec![
            message(&room, "alice", "rocket launch at noon", 1),
            message(&room, "bob", "on it", 2),
        ],
    );
    state.restore(&carol, vec![message(&carol, "carol", "are you there?", 3)]);
    assert_eq!(texts(&state, &room), ["rocket launch at noon", "on it"]);
    let held = state.buffer(&room).unwrap();
    assert_eq!((held.unread, held.highlights), (0, 0));
    assert_eq!(state.attention_count(), 0);
    assert!(state.remove(&carol));
    state.restore(&carol, vec![message(&carol, "carol", "again", 4)]);
    assert!(state.buffer(&carol).is_none());
    let elsewhere = subspace_client_irc::BufferKey {
        network: subspace_client_irc::NetworkId("elsewhere".into()),
        target: Target::Channel("#x".into()),
    };
    state.restore(&elsewhere, vec![message(&elsewhere, "x", "x", 1)]);
    assert!(state.buffer(&elsewhere).is_none());
}

#[test]
fn anchors_name_each_buffers_newest_message() {
    let mut state = state();
    let room = channel("#subspace");
    state.restore(
        &room,
        vec![
            message(&room, "alice", "first", 1),
            with_id(message(&room, "bob", "newest", 2), "n"),
        ],
    );
    let anchors = state.anchors(&network_id());
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].target, Target::Channel("#subspace".into()));
    assert_eq!(anchors[0].id.as_deref(), Some("n"));
    assert_eq!(anchors[0].time, at(2));
}

#[test]
fn a_replayed_copy_of_a_restored_line_is_not_added_again() {
    let mut state = state();
    let room = channel("#subspace");
    state.restore(&room, vec![message(&room, "alice", "hello", 1)]);
    state.apply(Event::Replayed(message(&room, "alice", "hello", 1)), at(5));
    assert_eq!(texts(&state, &room), ["hello"]);
}
