//! What happens as lines come and go: endings outlive their notice, an
//! older replay never replaces a newer stage, and progress whose request
//! is no longer held shows as an ordinary notice.

use chrono::{DateTime, Utc};
use subspace_client_irc::{Event, Line};

use super::{is_folded, joined, label, live, progress, request, room, ship};
use crate::fixtures::{at, message};
use crate::state::{Buffer, IrcState, LINE_CAP};
use crate::thread_status::shown_entry;

/// `count` ordinary messages from bob, each with its own message ID.
fn chatter(from: usize, count: usize) -> Vec<Line> {
    (from..from + count)
        .map(|n| {
            let mut line = message(&room(), "bob", &format!("line {n}"), 20);
            line.id = Some(format!("chat-{n}"));
            line
        })
        .collect()
}

#[test]
fn an_ending_survives_the_eviction_and_partial_reload_of_its_notice() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "tool", 2),
            progress(&room(), "first", "completed", 3),
        ],
    );
    live(&mut state, chatter(0, LINE_CAP));
    assert!(
        state
            .buffer(&room())
            .is_some_and(|held| held
                .entries
                .iter()
                .all(|entry| entry.line.id.as_deref() != Some("first"))),
        "the request and its notices were dropped"
    );
    state.apply(
        Event::Backfill {
            buffer: room(),
            lines: vec![
                request(&room(), "first", 1),
                progress(&room(), "first", "tool", 2),
            ],
        },
        at(31),
    );
    assert!(is_folded(&state, &room(), "astra-first-2"));
    assert_eq!(
        label(&state, "first"),
        None,
        "without its completed notice, the request stays ended"
    );
}

#[test]
fn an_older_replayed_stage_does_not_replace_a_newer_one() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "waiting", 5),
        ],
    );
    state.apply(
        Event::Replayed(progress(&room(), "first", "tool", 3)),
        at(30),
    );
    assert!(is_folded(&state, &room(), "astra-first-3"));
    assert_eq!(label(&state, "first"), Some("Waiting for your answer"));
    live(&mut state, [progress(&room(), "first", "clarified", 6)]);
    assert_eq!(label(&state, "first"), Some("Answer received"));
}

/// The room after its request "first" was dropped by the line cap while
/// two folded notices on it, the second a mention, stayed held, read up to
/// `marker` when one is given.
fn dropped_request(marker: Option<DateTime<Utc>>) -> IrcState {
    let mut state = joined(vec![ship()]);
    if let Some(time) = marker {
        state.apply(
            Event::ReadMarker {
                buffer: room(),
                time,
            },
            at(0),
        );
    }
    live(&mut state, [request(&room(), "first", 1)]);
    live(&mut state, chatter(0, LINE_CAP - 3));
    let mut mention = progress(&room(), "first", "reply", 31);
    mention.highlight = true;
    live(
        &mut state,
        [progress(&room(), "first", "tool", 30), mention],
    );
    assert!(is_folded(&state, &room(), "astra-first-31"));
    let highlights = state.buffer(&room()).map(|held| held.highlights);
    assert_eq!(highlights, Some(0), "folded progress never counts");
    live(&mut state, chatter(LINE_CAP, 1));
    state
}

/// The sequence and position of the line with message ID `id`.
fn held_at(held: &Buffer, id: &str) -> (usize, i64) {
    let position = held
        .entries
        .iter()
        .position(|entry| entry.line.id.as_deref() == Some(id))
        .unwrap();
    (position, held.entries[position].seq)
}

#[test]
fn progress_whose_request_is_dropped_shows_without_new_unread_activity() {
    let state = dropped_request(None);
    for id in ["astra-first-30", "astra-first-31"] {
        assert!(!is_folded(&state, &room(), id), "{id}: request not held");
    }
    let held = state.buffer(&room()).unwrap();
    let (position, seq) = held_at(held, "astra-first-31");
    assert!(
        !held.unread_entries.contains(&seq),
        "without a read marker, restored progress is not news"
    );
    assert_eq!(held.highlights, 0);
    assert_eq!(
        shown_entry(held, seq),
        Some((position, seq)),
        "Search reveals the notice itself"
    );
}

#[test]
fn progress_whose_request_is_dropped_counts_against_a_read_marker() {
    let state = dropped_request(Some(at(25)));
    let held = state.buffer(&room()).unwrap();
    for id in ["astra-first-30", "astra-first-31"] {
        let (_, seq) = held_at(held, id);
        assert!(held.unread_entries.contains(&seq), "{id} is newer");
    }
    assert_eq!(held.highlights, 1, "the mention highlights at once");
}
