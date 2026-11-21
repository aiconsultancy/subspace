//! Each request is pinned to the account its agent held when Channels
//! first saw it with one: whoever holds the nick later, only that account's
//! notices fold, and member events never change a pinned request.

use subspace_client_irc::{Event, Member};

use super::{is_folded, joined, label, live, notice_from, progress, request, room, ship};
use crate::fixtures::{at, member, message};

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
fn a_member_event_does_not_change_a_pinned_request() {
    let mut state = joined(vec![member("astra", "+")]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "tool", 2),
        ],
    );
    assert!(
        !is_folded(&state, &room(), "astra-first-2"),
        "no account yet"
    );
    state.apply(astra_changed(ship()), at(31));
    assert!(
        is_folded(&state, &room(), "astra-first-2"),
        "pinned when the server first reports an account"
    );
    assert_eq!(state.buffer(&room()).map(|held| held.unread), Some(0));
    state.apply(
        Event::Members {
            buffer: room(),
            members: vec![taker()],
        },
        at(32),
    );
    state.apply(astra_changed(member("astra", "")), at(33));
    assert!(is_folded(&state, &room(), "astra-first-2"));
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
