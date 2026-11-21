//! Each request is bound to one account: whoever holds the nick later,
//! only that account's notices fold. Member events neither bind a request
//! nor change its binding, and the requester's own account never binds.

use subspace_client_irc::{Event, Member};

use super::{SHIP, is_folded, joined, label, live, notice_from, progress, request, room, ship};
use crate::fixtures::{at, member, message};
use crate::state::IrcState;

/// The account request `id` in the room is bound to.
fn pin(state: &IrcState, id: &str) -> Option<String> {
    state.buffer(&room())?.requests.pin(id).map(str::to_owned)
}

/// Another account holding the ship's nick.
fn taker() -> Member {
    let mut taker = member("astra", "+");
    taker.account = Some("mallory".into());
    taker
}

/// The server's new facts about astra, such as its account.
fn astra_changed(member: Member) -> Event {
    Event::MemberChanged {
        buffer: room(),
        old_nick: "astra".into(),
        member,
    }
}

#[test]
fn a_nick_taken_over_by_another_account_cannot_fold_spoof_or_end_a_pinned_request() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "tool", 2),
        ],
    );
    state.apply(
        Event::MemberLeft {
            buffer: room(),
            nick: "astra".into(),
        },
        at(30),
    );
    state.apply(
        Event::MemberJoined {
            buffer: room(),
            member: taker(),
        },
        at(30),
    );
    live(
        &mut state,
        [
            notice_from(("astra", Some("mallory")), &room(), "first", "waiting", 3),
            notice_from(("astra", Some("mallory")), &room(), "first", "completed", 4),
        ],
    );
    for id in ["astra-first-3", "astra-first-4"] {
        assert!(!is_folded(&state, &room(), id), "{id} stays a notice");
    }
    assert_eq!(
        label(&state, "first"),
        Some("Using a tool"),
        "the new holder neither changed nor ended it"
    );
    assert_eq!(state.buffer(&room()).map(|held| held.unread), Some(2));
    state.apply(
        Event::History {
            buffer: room(),
            lines: Vec::new(),
            complete: true,
        },
        at(31),
    );
    assert!(!is_folded(&state, &room(), "astra-first-4"), "folded again");
    live(&mut state, [progress(&room(), "first", "reply", 5)]);
    assert!(is_folded(&state, &room(), "astra-first-5"));
    assert_eq!(
        label(&state, "first"),
        Some("Writing a reply"),
        "the pinned account still can"
    );
}

#[test]
fn a_request_that_asks_its_own_sender_folds_nothing() {
    let mut sender = member("mallory", "");
    sender.account = Some("mallory".into());
    let mut state = joined(vec![ship(), sender]);
    let mut own = message(&room(), "mallory", "MALLORY: deploy", 1);
    own.id = Some("self".into());
    live(
        &mut state,
        [
            own,
            notice_from(("mallory", Some("mallory")), &room(), "self", "waiting", 2),
        ],
    );
    assert!(!is_folded(&state, &room(), "mallory-self-2"));
    assert_eq!(label(&state, "self"), None);
    assert_eq!(
        state.buffer(&room()).map(|held| held.unread),
        Some(2),
        "the notice counts as any notice"
    );
}

#[test]
fn member_events_neither_bind_nor_change_a_request() {
    let mut state = joined(vec![member("astra", "+")]);
    live(&mut state, [request(&room(), "first", 1)]);
    state.apply(astra_changed(ship()), at(31));
    assert_eq!(pin(&state, "first"), None, "a member event does not bind");
    live(&mut state, [progress(&room(), "first", "tool", 2)]);
    assert!(
        is_folded(&state, &room(), "astra-first-2"),
        "the agent's first answer binds"
    );
    assert_eq!(pin(&state, "first").as_deref(), Some(SHIP));
    state.apply(
        Event::Members {
            buffer: room(),
            members: vec![taker()],
        },
        at(32),
    );
    state.apply(astra_changed(member("astra", "")), at(33));
    assert_eq!(pin(&state, "first").as_deref(), Some(SHIP));
    live(
        &mut state,
        [
            progress(&room(), "first", "reply", 3),
            notice_from(("astra", Some("mallory")), &room(), "first", "completed", 4),
        ],
    );
    assert!(is_folded(&state, &room(), "astra-first-3"));
    assert!(!is_folded(&state, &room(), "astra-first-4"));
    assert_eq!(label(&state, "first"), Some("Writing a reply"));
}

#[test]
fn a_request_to_another_nick_of_the_requesters_own_account_folds_nothing() {
    let mut twin = member("mallory", "");
    twin.account = Some("mallory".into());
    let mut state = joined(vec![twin]);
    let mut asked = message(&room(), "mallory2", "mallory: deploy", 1);
    asked.id = Some("twin".into());
    asked.account = Some("Mallory".into());
    live(
        &mut state,
        [
            asked,
            notice_from(("mallory", Some("mallory")), &room(), "twin", "waiting", 2),
        ],
    );
    assert_eq!(
        pin(&state, "twin"),
        None,
        "neither the member list nor the answer binds"
    );
    assert!(!is_folded(&state, &room(), "mallory-twin-2"));
    assert_eq!(label(&state, "twin"), None);
}
