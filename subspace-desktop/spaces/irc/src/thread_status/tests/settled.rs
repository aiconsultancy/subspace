//! A request's line while it runs, with its stage on the meter and the
//! time it was sent, and the quiet line it settles into: done or failed,
//! with how long it took, also after a restart, and plain `Done` for an
//! ending stored before its time was kept.

use chrono::TimeDelta;
use subspace_client_irc::Event;

use super::{
    connected, joined, label, live, progress, request, room, settled, ship, shown_line as line,
};
use crate::fixtures::{at, config, member};
use crate::state::IrcState;
use crate::thread_status::{Ending, Outcome, Settled, Stage};

#[test]
fn a_running_line_maps_stargate_s_stages_onto_the_meter() {
    let mut state = joined(vec![ship()]);
    live(&mut state, [request(&room(), "first", 1)]);
    for (minute, (value, stage)) in (2..).zip([
        ("received", Stage::Accepted),
        ("queued", Stage::Accepted),
        ("delivered", Stage::Accepted),
        ("reasoning", Stage::Working),
        ("tool", Stage::Working),
        ("retry", Stage::Working),
        ("waiting", Stage::Waiting),
        ("clarified", Stage::Waiting),
        ("reply", Stage::Reply),
    ]) {
        live(&mut state, [progress(&room(), "first", value, minute)]);
        let shown = line(&state, &room(), "first").unwrap();
        assert_eq!(shown.stage, stage, "{value}");
        assert_eq!(shown.since, at(1), "the clock counts from the request");
        assert!(shown.ticking(), "{value} is live");
    }
}

/// The room with our request "first" at minute 1, which astra ended with
/// `value` at `minute`.
fn ended(value: &str, minute: i64) -> IrcState {
    let mut state = joined(vec![ship()]);
    let lines = [
        request(&room(), "first", 1),
        progress(&room(), "first", value, minute),
    ];
    live(&mut state, lines);
    state
}

#[test]
fn a_completed_request_settles_into_done_with_its_duration() {
    let state = ended("completed", 43);
    assert_eq!(label(&state, "first"), None, "no running line");
    let shown = line(&state, &room(), "first").unwrap();
    assert_eq!(shown.stage, Stage::Done);
    assert!(!shown.ticking(), "a settled line does not tick");
    assert_eq!(
        shown.settled,
        Some(Settled {
            failed: false,
            took: Some(TimeDelta::minutes(42)),
        })
    );
    assert_eq!(shown.agent, "astra");
}

#[test]
fn a_failed_request_settles_into_failed_after_its_duration() {
    assert_eq!(
        settled(&ended("failed", 4), "first"),
        Some(Settled {
            failed: true,
            took: Some(TimeDelta::minutes(3)),
        })
    );
}

#[test]
fn each_stage_has_its_step_word_and_share_of_the_meter() {
    let steps: Vec<(usize, &str, f32)> = Stage::ALL
        .iter()
        .map(|stage| (stage.step(), stage.word(), stage.percent()))
        .collect();
    assert_eq!(
        steps,
        [
            (1, "accepted", 20.0),
            (2, "working", 40.0),
            (3, "waiting", 60.0),
            (4, "reply", 80.0),
            (5, "done", 100.0),
        ]
    );
}

/// After a restart with the room's stored `lines` and `ended`, rejoined
/// with the ship.
fn restarted(lines: Vec<subspace_client_irc::Line>, ended: &[(String, Ending)]) -> IrcState {
    let mut state = IrcState::new(&[config()], Vec::new());
    state.restore_buffer(&room(), lines, ended);
    state.apply(connected(), at(60));
    state.apply(Event::Joined { buffer: room() }, at(60));
    state.apply(
        Event::Members {
            buffer: room(),
            members: vec![ship(), member("bob", "")],
        },
        at(60),
    );
    state
}

#[test]
fn a_stored_ending_keeps_its_outcome_and_time_after_a_restart() {
    let ending = Ending {
        outcome: Some(Outcome::Failed),
        at: Some(at(11)),
    };
    let state = restarted(
        vec![request(&room(), "first", 1)],
        &[("first".to_owned(), ending)],
    );
    assert_eq!(
        settled(&state, "first"),
        Some(Settled {
            failed: true,
            took: Some(TimeDelta::minutes(10)),
        })
    );
    assert_eq!(
        line(&state, &room(), "first").map(|shown| shown.agent),
        Some("astra".to_owned()),
        "named after the nick the request asks"
    );
}

#[test]
fn an_older_ending_without_a_time_loads_as_done_without_a_duration() {
    let state = restarted(
        vec![request(&room(), "first", 1)],
        &[("first".to_owned(), Ending::default())],
    );
    assert_eq!(
        settled(&state, "first"),
        Some(Settled {
            failed: false,
            took: None,
        })
    );
}

#[test]
fn a_notice_fills_in_an_older_ending_once_for_the_store() {
    let mut state = restarted(
        vec![request(&room(), "first", 1)],
        &[("first".to_owned(), Ending::default())],
    );
    live(&mut state, [progress(&room(), "first", "completed", 5)]);
    let taken = state.take_endings();
    assert_eq!(
        taken,
        [(
            room(),
            vec![(
                "first".to_owned(),
                Ending {
                    outcome: Some(Outcome::Completed),
                    at: Some(at(5)),
                }
            )]
        )]
    );
    live(&mut state, [progress(&room(), "first", "failed", 6)]);
    assert!(state.take_endings().is_empty(), "the first outcome stands");
    assert_eq!(
        settled(&state, "first").map(|settled| settled.failed),
        Some(false)
    );
}

#[test]
fn a_stored_failure_stays_failed_when_a_restored_notice_says_completed() {
    let failed = Ending {
        outcome: Some(Outcome::Failed),
        at: Some(at(11)),
    };
    // The restored lines hold a later `completed` notice for the same
    // request; the ending seen live, and stored, still stands.
    let state = restarted(
        vec![
            request(&room(), "first", 1),
            progress(&room(), "first", "completed", 12),
        ],
        &[("first".to_owned(), failed)],
    );
    assert_eq!(
        settled(&state, "first").map(|settled| settled.failed),
        Some(true)
    );
}
