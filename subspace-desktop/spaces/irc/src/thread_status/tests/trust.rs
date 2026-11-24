//! Which progress notices the server vouches for, and how trusted and
//! untrusted notices count, live and from history.

use subspace_client_irc::{BufferKey, Event};

use super::{is_folded, joined, label, live, notice_from, progress, request, room, ship, status};
use crate::fixtures::{at, member, query};
use crate::state::{AttentionKind, IrcState};

#[test]
fn only_the_addressed_ships_server_account_is_trusted() {
    let mut state = joined(vec![ship(), member("nova", "")]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "tool", 2),
            notice_from(("nova", Some("nova")), &room(), "first", "completed", 3),
            notice_from(
                ("astra", Some("impostor")),
                &room(),
                "first",
                "completed",
                4,
            ),
            notice_from(("astra", None), &room(), "first", "completed", 5),
            progress(&room(), "unknown", "completed", 6),
            progress(&room(), "astra-first-2", "completed", 7),
        ],
    );
    assert!(is_folded(&state, &room(), "astra-first-2"));
    for id in [
        "nova-first-3",
        "astra-first-4",
        "astra-first-5",
        "astra-unknown-6",
        "astra-astra-first-2-7",
    ] {
        assert!(!is_folded(&state, &room(), id), "{id} stays a notice");
    }
    assert_eq!(
        label(&state, "first"),
        Some("Using a tool"),
        "none of them ended it"
    );
    assert_eq!(
        state.buffer(&room()).map(|held| held.unread),
        Some(5),
        "untrusted notices count as today"
    );
}

#[test]
fn a_request_without_an_addressee_or_a_notice_without_an_account_folds_nothing() {
    let mut unaddressed = joined(vec![ship()]);
    let mut plain = request(&room(), "first", 1);
    plain.text = "please handle first".into();
    live(
        &mut unaddressed,
        [plain, progress(&room(), "first", "tool", 2)],
    );
    assert!(!is_folded(&unaddressed, &room(), "astra-first-2"));
    assert_eq!(label(&unaddressed, "first"), None);

    let mut unvouched = joined(vec![member("astra", "+")]);
    live(
        &mut unvouched,
        [
            request(&room(), "first", 1),
            notice_from(("astra", None), &room(), "first", "tool", 2),
        ],
    );
    assert!(
        !is_folded(&unvouched, &room(), "astra-first-2"),
        "a notice without an account tag"
    );
    assert_eq!(label(&unvouched, "first"), None);

    let mut mentioned = joined(vec![ship()]);
    let mut at_mention = request(&room(), "first", 1);
    at_mention.text = "@astra please handle first".into();
    live(
        &mut mentioned,
        [at_mention, progress(&room(), "first", "tool", 2)],
    );
    assert!(
        is_folded(&mentioned, &room(), "astra-first-2"),
        "@nick addresses too"
    );
}

#[test]
fn inline_mentions_fold_only_for_one_distinct_addressed_account() {
    for (text, folds) in [
        ("Could you check this, @ASTRA?", true),
        ("Thanks, @astra. Could you check this?", true),
        ("@astra. Please ask @nova to confirm.", true),
        ("astra: Please ask @nova to confirm.", true),
        ("astra, please ask @nova to confirm.", true),
        (
            "Could you check https://social.example.com/@nova, @astra?",
            true,
        ),
        ("See https://social.example.com/@astra.", false),
        ("Ask (@astra) or wait for @ASTRA.", true),
        ("Compare @astra and @nova.", false),
        ("Contact pilot@astra.example.", false),
    ] {
        let mut state = joined(vec![ship(), member("nova", "")]);
        let mut line = request(&room(), "first", 1);
        line.text = text.into();
        live(
            &mut state,
            [
                line,
                progress(&room(), "first", "tool", 2),
                notice_from(("nova", Some("nova")), &room(), "first", "completed", 3),
            ],
        );
        assert_eq!(is_folded(&state, &room(), "astra-first-2"), folds, "{text}");
        assert!(!is_folded(&state, &room(), "nova-first-3"), "{text}");
    }
}

#[test]
fn trusted_progress_never_counts_or_notifies_but_a_lookalike_does() {
    let private = query("astra");
    let mut state = joined(vec![ship(), member("nova", "")]);
    state.apply(Event::Line(request(&private, "direct", 1)), at(30));
    let trusted = state.apply(Event::Line(progress(&private, "direct", "tool", 2)), at(30));
    assert_eq!(trusted, None);
    assert_eq!(state.buffer(&private).map(|held| held.unread), Some(0));
    assert_eq!(label_in(&state, &private, "direct"), Some("Using a tool"));
    let lookalike = notice_from(
        ("astra", Some("impostor")),
        &private,
        "direct",
        "completed",
        3,
    );
    let attention = state.apply(Event::Line(lookalike), at(30));
    assert_eq!(
        attention.map(|attention| attention.kind),
        Some(AttentionKind::Query)
    );
    assert_eq!(state.buffer(&private).map(|held| held.unread), Some(1));
    assert_eq!(label_in(&state, &private, "direct"), Some("Using a tool"));
}

fn label_in(state: &IrcState, buffer: &BufferKey, request: &str) -> Option<&'static str> {
    status(state, buffer, request).map(|shown| shown.label)
}

#[test]
fn history_progress_is_taken_out_of_unread_as_it_folds() {
    let mut state = joined(vec![member("astra", "+")]);
    state.apply(
        Event::ReadMarker {
            buffer: room(),
            time: at(0),
        },
        at(0),
    );
    state.apply(
        Event::History {
            buffer: room(),
            lines: vec![
                request(&room(), "first", 1),
                progress(&room(), "first", "tool", 2),
                notice_from(("nova", Some("nova")), &room(), "first", "completed", 3),
            ],
            complete: true,
        },
        at(30),
    );
    assert_eq!(
        state.buffer(&room()).map(|held| held.unread),
        Some(1),
        "the agent's progress folds; the lookalike counts"
    );
    assert!(is_folded(&state, &room(), "astra-first-2"));
    assert!(!is_folded(&state, &room(), "nova-first-3"));
    state.apply(
        Event::Members {
            buffer: room(),
            members: vec![ship(), member("nova", "")],
        },
        at(31),
    );
    assert_eq!(state.buffer(&room()).map(|held| held.unread), Some(1));
    assert_eq!(label(&state, "first"), Some("Using a tool"));
}
