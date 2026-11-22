//! Endings are taken for the store once, and a stored ending keeps a
//! finished request finished after a restart, also when none of its
//! buffer's lines survive. The trust rule holds after a restart too.

use subspace_client_irc::{BufferKey, Event, Line};

use super::{connected, is_folded, joined, label, live, notice_from, progress, request, room};
use crate::fixtures::{at, config, member};
use crate::state::IrcState;

/// After a restart: the stored `lines` and endings of the room, then the
/// room joined with another account holding astra's nick.
fn restarted(room: &BufferKey, lines: Vec<Line>, ended: &[String]) -> IrcState {
    let mut state = IrcState::new(&[config()], Vec::new());
    state.restore_buffer(room, lines, ended);
    state.apply(connected(), at(30));
    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(30),
    );
    let mut taker = member("astra", "+");
    taker.account = Some("mallory".into());
    state.apply(
        Event::Members {
            buffer: room.clone(),
            members: vec![taker],
        },
        at(30),
    );
    state
}

/// The endings `state` hands the store for the room.
fn taken(state: &mut IrcState) -> Vec<String> {
    state
        .take_endings()
        .into_iter()
        .find(|(key, _)| *key == room())
        .map(|(_, ended)| ended)
        .unwrap_or_default()
}

#[test]
fn an_ending_is_taken_for_the_store_once() {
    let mut state = joined(vec![super::ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "completed", 2),
            notice_from(("nova", Some("nova")), &room(), "second", "completed", 3),
        ],
    );
    assert_eq!(taken(&mut state), ["first"], "only trusted endings");
    assert!(taken(&mut state).is_empty(), "taken once");
}

#[test]
fn after_a_restart_a_nick_taken_over_cannot_fold_spoof_or_end_a_request() {
    let room = room();
    let lines = vec![
        request(&room, "first", 1),
        progress(&room, "first", "tool", 2),
    ];
    let mut state = restarted(&room, lines, &[]);
    live(
        &mut state,
        [
            notice_from(("astra", Some("mallory")), &room, "first", "waiting", 3),
            notice_from(("astra", Some("mallory")), &room, "first", "completed", 4),
        ],
    );
    assert!(!is_folded(&state, &room, "astra-first-3"));
    assert!(!is_folded(&state, &room, "astra-first-4"));
    assert_eq!(label(&state, "first"), Some("Using a tool"));
    live(&mut state, [progress(&room, "first", "reply", 5)]);
    assert!(is_folded(&state, &room, "astra-first-5"));
    assert_eq!(label(&state, "first"), Some("Writing a reply"));
}

#[test]
fn a_stored_ending_keeps_a_request_finished_with_no_surviving_line() {
    let room = room();
    let mut state = restarted(&room, Vec::new(), &["first".to_owned()]);
    assert!(
        state
            .buffer(&room)
            .is_some_and(|held| held.entries.is_empty()),
        "the ending alone added no line"
    );
    state.apply(
        Event::History {
            buffer: room.clone(),
            lines: vec![
                request(&room, "first", 1),
                progress(&room, "first", "tool", 2),
            ],
            complete: true,
        },
        at(31),
    );
    assert!(is_folded(&state, &room, "astra-first-2"));
    assert_eq!(
        label(&state, "first"),
        None,
        "its completed notice did not reload"
    );
}

#[test]
fn endings_for_a_buffer_without_lines_do_not_bring_it_back() {
    let mut state = IrcState::new(&[config()], Vec::new());
    state.restore_buffer(&room(), Vec::new(), &["first".to_owned()]);
    assert!(state.buffer(&room()).is_none());
}
