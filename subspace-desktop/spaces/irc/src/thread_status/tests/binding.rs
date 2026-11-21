//! A request seen without its agent's account binds to the account on the
//! earliest notice from the nick it asks, and a stored binding and ending
//! come back after a restart before anything else can bind.

use subspace_client_irc::{BufferKey, Event};

use super::{
    SHIP, connected, is_folded, joined, label, live, notice_from, progress, request, room,
};
use crate::fixtures::{at, config, member};
use crate::state::IrcState;
use crate::thread_status::Kept;

/// The account request `id` in the room is bound to.
fn pin(state: &IrcState, id: &str) -> Option<String> {
    state.buffer(&room())?.requests.pin(id).map(str::to_owned)
}

#[test]
fn a_request_first_seen_from_history_binds_to_the_earliest_answer_of_its_agent() {
    let mut state = joined(vec![member("astra", "+")]);
    state.apply(
        Event::History {
            buffer: room(),
            lines: vec![
                request(&room(), "first", 1),
                notice_from(("nova", Some("nova")), &room(), "first", "completed", 2),
                notice_from(("astra", Some("mallory")), &room(), "first", "waiting", 5),
                progress(&room(), "first", "tool", 3),
            ],
            complete: true,
        },
        at(30),
    );
    assert_eq!(pin(&state, "first").as_deref(), Some(SHIP));
    assert!(is_folded(&state, &room(), "astra-first-3"));
    assert!(
        !is_folded(&state, &room(), "astra-first-5"),
        "a later answer from another account"
    );
    assert!(
        !is_folded(&state, &room(), "nova-first-2"),
        "an earlier notice from another nick"
    );
    assert_eq!(label(&state, "first"), Some("Using a tool"));
}

#[test]
fn a_notice_from_another_nick_or_a_later_account_neither_binds_nor_folds() {
    let mut state = joined(vec![member("astra", "+")]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            notice_from(("nova", Some("nova")), &room(), "first", "tool", 2),
        ],
    );
    assert!(!is_folded(&state, &room(), "nova-first-2"));
    assert_eq!(pin(&state, "first"), None);
    live(
        &mut state,
        [
            progress(&room(), "first", "tool", 3),
            notice_from(("astra", Some("mallory")), &room(), "first", "completed", 4),
        ],
    );
    assert!(is_folded(&state, &room(), "astra-first-3"));
    assert!(!is_folded(&state, &room(), "astra-first-4"));
    assert_eq!(label(&state, "first"), Some("Using a tool"));
}

/// What the store kept of the room's requests in `state`, through JSON.
fn stored(state: &mut IrcState) -> Vec<Kept> {
    let kept = state
        .take_kept()
        .into_iter()
        .find(|(key, _)| *key == room())
        .map(|(_, kept)| kept)
        .unwrap_or_default();
    let payload = serde_json::to_string(&kept).unwrap();
    serde_json::from_str(&payload).unwrap()
}

/// After a restart: the stored requests, then some of the room's lines,
/// then a member list where another account holds astra's nick.
fn restarted(kept: &[Kept], room: &BufferKey) -> IrcState {
    let mut state = IrcState::new(&[config()], Vec::new());
    state.restore_requests(room, kept);
    state.restore(
        room,
        vec![
            request(room, "first", 1),
            request(room, "second", 3),
            progress(room, "second", "reasoning", 4),
        ],
    );
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

#[test]
fn a_stored_binding_and_ending_survive_a_restart_and_a_nick_taken_over() {
    let mut before = joined(vec![super::ship()]);
    live(
        &mut before,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "tool", 2),
            request(&room(), "second", 3),
            progress(&room(), "second", "reasoning", 4),
            progress(&room(), "second", "completed", 5),
        ],
    );
    let kept = stored(&mut before);
    assert!(
        kept.iter().any(|kept| kept.request == "first"
            && kept.pin.as_deref() == Some(SHIP)
            && !kept.ended)
    );
    assert!(
        kept.iter()
            .any(|kept| kept.request == "second" && kept.ended)
    );
    assert!(before.take_kept().is_empty(), "taken once");
    let mut state = restarted(&kept, &room());
    assert_eq!(
        label(&state, "second"),
        None,
        "ended, although its completed notice did not restore"
    );
    live(
        &mut state,
        [
            notice_from(("astra", Some("mallory")), &room(), "first", "waiting", 6),
            notice_from(("astra", Some("mallory")), &room(), "first", "completed", 7),
        ],
    );
    assert!(!is_folded(&state, &room(), "astra-first-6"));
    assert!(!is_folded(&state, &room(), "astra-first-7"));
    assert_eq!(label(&state, "first"), None, "no progress of its own yet");
    live(&mut state, [progress(&room(), "first", "reply", 8)]);
    assert!(is_folded(&state, &room(), "astra-first-8"));
    assert_eq!(label(&state, "first"), Some("Writing a reply"));
}

#[test]
fn an_older_stored_request_without_a_binding_or_ending_loads_with_none() {
    let kept: Kept = serde_json::from_str(r#"{"request":"first"}"#).unwrap();
    assert_eq!(
        kept,
        Kept {
            request: "first".into(),
            pin: None,
            ended: false,
        }
    );
}
