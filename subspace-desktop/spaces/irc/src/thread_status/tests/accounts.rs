//! A notice folds only when its server account equals the nick its request
//! asks: whoever holds the nick, other accounts never fold, and member
//! lists play no part.

use subspace_client_irc::{Event, Member};

use super::{is_folded, joined, label, live, notice_from, progress, request, room, ship};
use crate::fixtures::{at, member, message};

/// Another account holding the ship's nick.
fn taker() -> Member {
    let mut taker = member("astra", "+");
    taker.account = Some("mallory".into());
    taker
}

#[test]
fn a_notice_from_the_account_named_after_the_nick_folds_in_any_case() {
    let mut state = joined(Vec::new());
    let mut asked = request(&room(), "first", 1);
    asked.text = "Astra: please handle first".into();
    live(
        &mut state,
        [
            asked,
            notice_from(("astra", Some("ASTRA")), &room(), "first", "tool", 2),
        ],
    );
    assert!(
        is_folded(&state, &room(), "astra-first-2"),
        "no member list needed"
    );
    assert_eq!(label(&state, "first"), Some("Using a tool"));
}

#[test]
fn another_account_or_no_account_never_folds() {
    let mut state = joined(vec![ship(), member("nova", "")]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            notice_from(("nova", Some("nova")), &room(), "first", "completed", 2),
            notice_from(("astra", None), &room(), "first", "completed", 3),
        ],
    );
    assert!(!is_folded(&state, &room(), "nova-first-2"));
    assert!(!is_folded(&state, &room(), "astra-first-3"));
    assert_eq!(label(&state, "first"), None);
}

#[test]
fn a_nick_taken_over_by_another_account_cannot_fold_spoof_or_end_a_request() {
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
    assert_eq!(label(&state, "first"), Some("Using a tool"));
    assert_eq!(state.buffer(&room()).map(|held| held.unread), Some(2));
    live(&mut state, [progress(&room(), "first", "reply", 5)]);
    assert!(is_folded(&state, &room(), "astra-first-5"));
    assert_eq!(label(&state, "first"), Some("Writing a reply"));
}

#[test]
fn a_request_to_its_own_sender_by_nick_or_account_folds_nothing() {
    let mut state = joined(vec![ship()]);
    let mut by_nick = message(&room(), "mallory", "MALLORY: deploy", 1);
    by_nick.id = Some("self".into());
    let mut by_account = message(&room(), "astra2", "astra: deploy", 2);
    by_account.id = Some("twin".into());
    by_account.account = Some("Astra".into());
    live(
        &mut state,
        [
            by_nick,
            by_account,
            notice_from(("mallory", Some("mallory")), &room(), "self", "waiting", 3),
            progress(&room(), "twin", "waiting", 4),
        ],
    );
    assert!(!is_folded(&state, &room(), "mallory-self-3"));
    assert!(
        !is_folded(&state, &room(), "astra-twin-4"),
        "another nick of the account the request asks"
    );
    assert_eq!(label(&state, "self"), None);
    assert_eq!(label(&state, "twin"), None);
}

#[test]
fn a_replayed_old_request_folds_only_its_agents_progress() {
    let mut state = joined(vec![taker()]);
    for line in [
        request(&room(), "first", 1),
        notice_from(("astra", Some("mallory")), &room(), "first", "completed", 2),
        progress(&room(), "first", "tool", 3),
    ] {
        state.apply(Event::Replayed(line), at(30));
    }
    assert!(!is_folded(&state, &room(), "astra-first-2"));
    assert!(is_folded(&state, &room(), "astra-first-3"));
    assert_eq!(label(&state, "first"), Some("Using a tool"));
}
