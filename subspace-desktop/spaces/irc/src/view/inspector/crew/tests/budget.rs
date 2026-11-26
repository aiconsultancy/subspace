//! Response-size budgets captured per crew read.

use std::time::Duration;

use subspace_client_fleet::{
    Error, crew_detail_json_limit,
    model::{CrewContext, CrewMember},
};

use super::{DetailState, Roster, assert_progress, member, roster};

#[test]
fn a_growing_crew_cap_gets_one_follow_up_without_general_invalid_response_retries() {
    let full = "Sailing north beyond the markers. ".repeat(1000);
    for outcome in [Ok(two_contexts(&full)), Err(Error::InvalidResponse)] {
        let mut state = DetailState::new(roster("first", "Working…", 1), 0);
        let flight = state.begin().unwrap();
        assert!(state.update(Roster::Present(Box::new(two_contexts("Working…"))), 0));
        assert_eq!(state.begin(), None, "no concurrent crew read");
        state.finish(flight, Err(Error::InvalidResponse));
        assert_eq!(state.retry_delay(), None);
        let follow_up = state.begin().expect("the larger cap needs one follow-up");
        assert_eq!(state.begin(), None, "the follow-up is in flight");
        let expected = if outcome.is_ok() { &full } else { "Working…" };
        state.finish(follow_up, outcome);
        assert_progress(&state, expected);
        assert_eq!(state.retry_delay(), None);
        assert_eq!(state.begin(), None, "no retry at the same cap");
    }
}

fn two_contexts(progress: &str) -> CrewMember {
    let mut crew = member("first", progress, 1);
    let mut idle = CrewContext::default();
    idle.project = Some("harbour".into());
    crew.contexts.push(idle);
    crew
}

#[test]
fn each_crew_cap_uses_its_own_roster_snapshot() {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let flight = state.begin().unwrap();
    assert!(state.update(Roster::Present(Box::new(two_contexts("Working…"))), 0));
    state.finish(flight, Err(Error::Busy(Some(31))));
    assert_eq!(
        state.begin(),
        None,
        "Retry-After still holds the larger read"
    );
    assert_eq!(state.retry_delay(), Some(Duration::from_secs(31)));
    state.retry_elapsed();
    let follow_up = state
        .begin()
        .expect("read at the larger cap after the hold");
    assert_eq!(crew_detail_json_limit(flight.contexts), 16 << 20);
    assert_eq!(crew_detail_json_limit(follow_up.contexts), 32 << 20);
    state.finish(follow_up, Err(Error::NotFound));
    assert_eq!(state.begin(), None, "404 never polls");
}
