//! History prepending, its cap and `msgid` de-duplication.

use subspace_client_irc::{BufferKey, Event, Line, LineKind};

use super::super::{HISTORY_MAX, HistoryState, IrcState, LINE_CAP};
use super::state;
use crate::fixtures::{at, channel, line, message, query};

/// One history page of `count` lines ending a minute before `end`.
fn page(room: &BufferKey, end: i64, count: i64) -> Vec<Line> {
    (end - count..end)
        .map(|minute| line(room, LineKind::Privmsg, "alice", "old", minute))
        .collect()
}

#[test]
fn history_is_prepended_without_duplicates_and_ends_loading() {
    let mut state = state();
    let room = channel("#subspace");
    let mut latest = message(&room, "alice", "latest", 10);
    latest.id = Some("m10".into());
    state.apply(Event::Line(latest.clone()), at(10));
    assert!(state.begin_history(&room));
    assert!(!state.begin_history(&room), "one request at a time");
    let mut older = message(&room, "bob", "older", 5);
    older.id = Some("m5".into());
    state.apply(
        Event::History {
            buffer: room.clone(),
            lines: vec![older, latest],
            complete: true,
        },
        at(11),
    );
    let buffer = state.buffer(&room).unwrap();
    let texts: Vec<_> = buffer
        .entries
        .iter()
        .map(|entry| entry.line.text.as_str())
        .collect();
    assert_eq!(texts, ["older", "latest"]);
    assert!(buffer.entries[0].seq < buffer.entries[1].seq);
    assert_eq!(buffer.history.state, HistoryState::Complete);
    assert!(!state.begin_history(&room), "the server has no more");
}

#[test]
fn loaded_history_survives_live_lines_up_to_the_maximum() {
    let mut state = state();
    let room = channel("#busy");
    let start = 100_000;
    for minute in start..start + LINE_CAP as i64 {
        state.apply(
            Event::Line(message(&room, "bob", "live", minute)),
            at(minute),
        );
    }
    let history = |state: &mut IrcState, end, count| {
        assert!(state.begin_history(&room));
        state.apply(
            Event::History {
                buffer: room.clone(),
                lines: page(&room, end, count),
                complete: false,
            },
            at(end),
        );
    };
    history(&mut state, start, 100);
    let newest = start + LINE_CAP as i64;
    state.apply(
        Event::Line(message(&room, "bob", "live", newest)),
        at(newest),
    );
    let buffer = state.buffer(&room).unwrap();
    assert_eq!(buffer.entries.len(), LINE_CAP + 100);
    assert_eq!(
        buffer.entries[0].line.time,
        at(start - 99),
        "one oldest line made room"
    );
    let mut end = start - 100;
    while state.buffer(&room).unwrap().entries.len() + 100 <= HISTORY_MAX {
        history(&mut state, end, 100);
        end -= 100;
    }
    history(&mut state, end, 100);
    let buffer = state.buffer(&room).unwrap();
    assert_eq!(buffer.entries.len(), HISTORY_MAX);
    assert_eq!(
        buffer.history.state,
        HistoryState::Complete,
        "a full buffer offers no older history"
    );
    assert!(
        buffer
            .entries
            .iter()
            .zip(buffer.entries.iter().skip(1))
            .all(|(a, b)| a.seq < b.seq)
    );
    assert!(!state.begin_history(&room));
}

#[test]
fn repeated_msgids_in_one_history_page_are_kept_once() {
    let mut state = state();
    let room = channel("#subspace");
    let mut first = message(&room, "alice", "once", 1);
    first.id = Some("m1".into());
    let mut repeat = message(&room, "alice", "once", 1);
    repeat.id = Some("m1".into());
    state.apply(
        Event::History {
            buffer: room.clone(),
            lines: vec![first, repeat],
            complete: false,
        },
        at(2),
    );
    assert_eq!(state.buffer(&room).unwrap().entries.len(), 1);
}

#[test]
fn an_unavailable_history_answer_hides_the_older_messages_row() {
    let mut state = state();
    let room = channel("#subspace");
    state.apply(Event::Line(message(&room, "alice", "hello", 1)), at(1));
    let rows = |state: &IrcState| {
        crate::transcript::rows(state.buffer(&room).unwrap(), &room.target, &chrono::Utc)
    };
    assert_eq!(rows(&state)[0], crate::transcript::Row::History);
    assert!(state.begin_history(&room));
    state.apply(
        Event::History {
            buffer: room.clone(),
            lines: Vec::new(),
            complete: true,
        },
        at(2),
    );
    let buffer = state.buffer(&room).unwrap();
    assert_eq!(buffer.history.state, HistoryState::Complete);
    assert!(!rows(&state).contains(&crate::transcript::Row::History));
}

#[test]
fn a_repeated_msgid_is_shown_once_and_a_query_open_is_reported() {
    let mut state = state();
    let room = channel("#subspace");
    let mut join = line(&room, LineKind::Join, "subspace", "joined", 0);
    join.id = Some("j1".into());
    state.apply(Event::Line(join.clone()), at(0));
    state.apply(Event::Line(join.clone()), at(0));
    state.apply(
        Event::History {
            buffer: room.clone(),
            lines: vec![join],
            complete: false,
        },
        at(1),
    );
    assert_eq!(state.buffer(&room).unwrap().entries.len(), 1);
    state.apply(
        Event::Joined {
            buffer: query("dave"),
        },
        at(2),
    );
    assert_eq!(state.take_opened(), Some(query("dave")));
    assert_eq!(state.take_opened(), None);
    state.apply(Event::Joined { buffer: room }, at(3));
    assert_eq!(
        state.take_opened(),
        None,
        "channels do not take the selection"
    );
}

#[test]
fn an_ended_history_request_returns_to_idle_and_can_be_repeated() {
    let mut state = state();
    let room = channel("#subspace");
    state.apply(Event::Line(message(&room, "alice", "hello", 1)), at(1));
    assert!(state.begin_history(&room));
    state.apply(
        Event::History {
            buffer: room.clone(),
            lines: Vec::new(),
            complete: false,
        },
        at(2),
    );
    assert_eq!(
        state.buffer(&room).unwrap().history.state,
        HistoryState::Idle
    );
    assert!(state.begin_history(&room), "the request may be repeated");
}
