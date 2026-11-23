//! One status per request: updates, questions, endings, staleness,
//! reopening and Search.

use subspace_client_irc::{ConnectionStatus, Event};

use super::{
    connected, follow_up, is_folded, joined, label, live, progress, request, room, said, ship,
    shown, status, status_event,
};
use crate::fixtures::{at, config, message};
use crate::state::IrcState;
use crate::thread_status::{PROGRESS_TAG, shown_entry};

#[test]
fn two_requests_including_two_in_one_thread_keep_separate_lines() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            request(&room(), "second", 2),
            follow_up(&room(), "third", "first", 3),
            progress(&room(), "first", "reasoning", 4),
            progress(&room(), "second", "queued", 5),
            progress(&room(), "third", "submitting", 6),
        ],
    );
    let before = (label(&state, "second"), label(&state, "third"));
    live(&mut state, [progress(&room(), "first", "tool", 7)]);
    assert_eq!(label(&state, "first"), Some("Using a tool"));
    assert_eq!(
        (label(&state, "second"), label(&state, "third")),
        before,
        "an update to one request leaves the others alone"
    );
    assert_eq!(before, (Some("Queued"), Some("Receipt unconfirmed")));
    live(&mut state, [progress(&room(), "third", "completed", 8)]);
    assert_eq!(
        label(&state, "third"),
        None,
        "only the matching request ends"
    );
    assert_eq!(label(&state, "first"), Some("Using a tool"));
    assert_eq!(label(&state, "second"), Some("Queued"));
}

#[test]
fn a_question_waits_for_its_answer_and_stays_a_message() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "waiting", 2),
            said(&room(), Some("first"), "question", 3),
        ],
    );
    assert_eq!(label(&state, "first"), Some("Waiting for your answer"));
    assert!(!is_folded(&state, &room(), "question"));
    let mut answer = message(&room(), "subspace", "astra: answer Q1 1", 4);
    answer.reply_to = Some("first".into());
    live(&mut state, [answer]);
    assert_eq!(label(&state, "first"), Some("Waiting for your answer"));
    live(&mut state, [progress(&room(), "first", "clarified", 5)]);
    assert_eq!(label(&state, "first"), Some("Answer received"));
}

#[test]
fn final_replies_clear_nothing_until_the_matching_request_ends() {
    let mut state = joined(vec![ship()]);
    let mut tagged_reply = said(&room(), Some("first"), "tagged-reply", 4);
    tagged_reply
        .client_tags
        .insert(PROGRESS_TAG.into(), "completed".into());
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "reply", 2),
            said(&room(), None, "main-room-final", 3),
            tagged_reply,
        ],
    );
    assert_eq!(label(&state, "first"), Some("Writing a reply"));
    assert!(!is_folded(&state, &room(), "main-room-final"));
    assert!(!is_folded(&state, &room(), "tagged-reply"));
    live(&mut state, [progress(&room(), "first", "failed", 5)]);
    assert_eq!(label(&state, "first"), None);
}

#[test]
fn late_or_historical_progress_never_reopens_an_ended_request() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "completed", 3),
            progress(&room(), "first", "tool", 4),
        ],
    );
    assert_eq!(label(&state, "first"), None, "late progress");
    state.apply(
        Event::History {
            buffer: room(),
            lines: vec![progress(&room(), "first", "reasoning", 2)],
            complete: false,
        },
        at(30),
    );
    assert_eq!(label(&state, "first"), None, "older progress from history");
    assert!(is_folded(&state, &room(), "astra-first-2"));
}

#[test]
fn a_lost_connection_shows_the_last_stage_as_stale() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "tool", 2),
        ],
    );
    assert_eq!(
        status(&state, &room(), "first"),
        Some(shown("Using a tool", None))
    );
    state.apply(
        status_event(ConnectionStatus::Reconnecting {
            attempt: 1,
            delay_secs: 5,
            error: String::new(),
        }),
        at(30),
    );
    let stale = Some(shown("Using a tool", Some(at(2))));
    assert_eq!(status(&state, &room(), "first"), stale, "while down");
    state.apply(connected(), at(31));
    assert_eq!(
        status(&state, &room(), "first"),
        stale,
        "until the agent reports again"
    );
    live(&mut state, [progress(&room(), "first", "reply", 3)]);
    assert_eq!(
        status(&state, &room(), "first"),
        Some(shown("Writing a reply", None))
    );
    live(
        &mut state,
        [progress(&room(), "first", "tool,disconnected", 4)],
    );
    assert_eq!(
        status(&state, &room(), "first"),
        Some(shown("Using a tool", Some(at(4)))),
        "the agent lost its own event stream"
    );
}

#[test]
fn reopening_a_completed_thread_does_not_make_it_look_active() {
    let room = room();
    let finished = [
        request(&room, "first", 1),
        progress(&room, "first", "tool", 2),
        said(&room, None, "final", 3),
        progress(&room, "first", "completed", 4),
        request(&room, "second", 5),
        progress(&room, "second", "reasoning", 6),
    ];
    let payload = serde_json::to_string(&finished).unwrap();
    let mut state = IrcState::new(&[config()], Vec::new());
    state.restore(&room, serde_json::from_str(&payload).unwrap());
    state.apply(connected(), at(30));
    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(30),
    );
    state.apply(
        Event::Members {
            buffer: room.clone(),
            members: vec![ship()],
        },
        at(30),
    );
    assert_eq!(label(&state, "first"), None);
    assert_eq!(
        status(&state, &room, "second"),
        Some(shown("Thinking", Some(at(6)))),
        "unfinished work from an earlier session reads as stale"
    );
}

#[test]
fn a_search_hit_on_folded_progress_shows_its_request() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "tool", 2),
            said(&room(), Some("first"), "answer", 3),
        ],
    );
    let held = state.buffer(&room()).unwrap();
    let seq = |at: usize| held.entries[at].seq;
    assert_eq!(shown_entry(held, seq(1)), Some((0, seq(0))));
    assert_eq!(shown_entry(held, seq(2)), Some((2, seq(2))));
    assert_eq!(shown_entry(held, 999), None);
}
