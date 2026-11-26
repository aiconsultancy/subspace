use std::time::Duration;

use subspace_client_fleet::{Error, model::ShipDetail};

use super::{DetailState, Roster, assert_progress, member, roster};

#[gpui_kit::test]
fn only_the_selected_fleets_token_change_reprobes_a_missing_route(
    cx: &mut gpui_kit::TestAppContext,
) {
    use subspace_client_fleet::{FleetApi, FleetRecord, TokenStore};
    use subspace_framework::core::Secret;

    let fleets = vec![
        FleetRecord::new(
            "aurora",
            "Aurora",
            Some("https://aurora.example.com".into()),
        ),
        FleetRecord::new(
            "cygnus",
            "Cygnus",
            Some("https://cygnus.example.com".into()),
        ),
    ];
    let api = cx.update(|cx| FleetApi::install_new(TokenStore::memory(), fleets, cx));
    cx.run_until_parked();
    let revision = |cx: &mut gpui_kit::TestAppContext| {
        api.read_with(cx, |api, _| api.connection_revision("aurora"))
    };
    let initial = revision(cx);
    let current = || roster("first", "Working…", 1);
    let mut state = DetailState::new(current(), initial);
    let flight = state.begin().unwrap();
    state.finish(flight, Err(Error::NotFound));

    for (id, changed) in [("cygnus", false), ("aurora", true)] {
        let save = api.update(cx, |api, cx| {
            api.save_token(id, Secret::new("fixture-token"), cx)
        });
        cx.foreground_executor().block_test(save).unwrap();
        assert_eq!(state.update(current(), revision(cx)), changed);
        assert_eq!(state.begin().is_some(), changed);
    }
}

#[test]
fn each_show_requests_once_and_a_read_from_before_hiding_cannot_win() {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let mut requests = vec![state.begin().unwrap()];
    assert!(state.set_visible(false));
    assert_eq!(state.begin(), None);
    assert!(state.set_visible(true));
    requests.push(state.begin().unwrap());
    assert!(!state.set_visible(true));
    assert_eq!(state.begin(), None);
    state.finish(requests[1], Ok(member("first", "Reopened detail", 1)));
    state.finish(requests[0], Ok(member("first", "Obsolete detail", 1)));
    assert_progress(&state, "Reopened detail");

    // Reopen after a completed read also makes exactly one new request.
    assert!(state.set_visible(false));
    assert_eq!(state.begin(), None);
    assert!(state.set_visible(true));
    requests.push(state.begin().unwrap());
    assert_eq!(state.begin(), None);
    assert_eq!(requests.len(), 3);
}

#[test]
fn missing_route_uses_the_current_roster() {
    let ship: ShipDetail = serde_json::from_str(
        r#"{"id":"lyra","available":true,"crewMembers":[{"id":"wren","name":"Wren"}]}"#,
    )
    .unwrap();
    let mut state = DetailState::new(Roster::find(Some(ship), "wren"), 0);
    let first = state.begin().unwrap();
    state.finish(first, Err(Error::NotFound));
    let (shown, note) = state.shown(true);
    assert_eq!(shown.unwrap().name, "Wren");
    assert_eq!(
        note,
        Some(
            "Crew detail is unavailable. Showing the ship's roster data. Older Stargate gateways don't serve crew detail."
        )
    );
    assert!(!state.can_retry());
    assert_eq!(state.retry_delay(), None);
    state.queue();
    assert_eq!(state.begin(), None, "ordinary updates do not poll a 404");
    // Reload and reconnect both explicitly probe the route once.
    state.probe();
    let probe = state.begin().unwrap();
    state.finish(probe, Err(Error::NotFound));
    assert_eq!(state.begin(), None);
    assert_eq!(state.retry_delay(), None);
    assert!(state.set_visible(false));
    assert_eq!(state.begin(), None);
    assert!(state.set_visible(true));
    assert!(
        state.begin().is_some(),
        "showing the detail probes it again"
    );
}

#[test]
fn a_missing_member_gets_one_probe_when_it_returns() {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let first = state.begin().unwrap();
    state.finish(first, Err(Error::NotFound));
    assert!(state.update(roster("first", "Working…", 2), 0));
    assert_eq!(state.begin(), None);
    assert!(state.update(Roster::Gone, 0));
    assert_eq!(state.begin(), None);
    assert!(state.update(roster("first", "Working…", 3), 0));
    let probe = state.begin().unwrap();
    state.finish(probe, Ok(member("first", "Full detail on return", 3)));
    assert_progress(&state, "Full detail on return");
    assert_eq!(state.begin(), None);
}

fn read_with_queued_task_revision() -> (DetailState, super::super::state::CrewRead) {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let flight = state.begin().unwrap();
    assert!(state.update(revised_task(), 0));
    assert_eq!(state.begin(), None);
    (state, flight)
}

fn revised_task() -> Roster {
    let mut newer = member("first", "Working…", 1);
    newer.contexts[0].current.as_mut().unwrap().updated_at = Some(2);
    Roster::Present(Box::new(newer))
}

#[test]
fn a_queued_task_revision_survives_an_invalid_response() {
    let (mut state, flight) = read_with_queued_task_revision();
    state.finish(flight, Err(Error::InvalidResponse));
    assert_eq!(state.retry_delay(), None);
    let follow_up = state
        .begin()
        .expect("the newer task revision still needs a read");
    state.finish(follow_up, Ok(member("first", "Unbounded latest detail", 1)));
    assert_progress(&state, "Unbounded latest detail");
    assert_eq!(state.begin(), None);
}

#[test]
fn a_queued_task_revision_waits_for_the_gateway_retry_after() {
    let (mut state, flight) = read_with_queued_task_revision();
    state.finish(flight, Err(Error::Busy(Some(6))));
    assert_eq!(state.begin(), None);
    assert_eq!(state.retry_delay(), Some(std::time::Duration::from_secs(6)));
    state.retry_elapsed();
    let follow_up = state.begin().expect("read the revision after Retry-After");
    state.finish(
        follow_up,
        Ok(member("first", "Latest detail after waiting", 1)),
    );
    assert_progress(&state, "Latest detail after waiting");
    assert_eq!(state.begin(), None);
}

#[test]
fn a_missing_route_discards_the_queued_task_revision() {
    let (mut state, flight) = read_with_queued_task_revision();
    state.finish(flight, Err(Error::NotFound));
    assert_eq!(state.begin(), None);
    assert_eq!(state.retry_delay(), None);
}

fn state_after_five_retries() -> DetailState {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    for delay in [1, 2, 4, 8, 16] {
        let flight = state.begin().unwrap();
        state.finish(flight, Err(Error::Unreachable));
        assert_eq!(state.retry_delay(), Some(Duration::from_secs(delay)));
        state.retry_elapsed();
    }
    state
}

#[test]
fn a_later_task_revision_reads_after_the_retry_budget_is_spent() {
    let mut state = state_after_five_retries();
    let last = state.begin().unwrap();
    state.finish(last, Err(Error::Unreachable));
    assert_eq!(state.retry_delay(), None);
    assert_eq!(state.begin(), None, "no automatic retry without demand");
    assert!(state.update(revised_task(), 0));
    assert!(state.begin().is_some(), "the later revision starts a read");
}

#[test]
fn a_task_revision_waits_for_a_long_retry_after() {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let flight = state.begin().unwrap();
    state.finish(flight, Err(Error::Busy(Some(31))));
    assert!(state.update(revised_task(), 0));
    assert_eq!(state.retry_delay(), Some(Duration::from_secs(31)));
    assert_eq!(state.begin(), None);
    state.retry_elapsed();
    assert_eq!(state.retry_delay(), None);
    let follow_up = state.begin().expect("read the held revision once");
    state.finish(follow_up, Err(Error::Unreachable));
    assert_eq!(
        state.retry_delay(),
        Some(Duration::from_secs(1)),
        "the hold used no retry budget"
    );
}

#[test]
fn a_hold_without_queued_demand_does_not_read() {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let flight = state.begin().unwrap();
    state.finish(flight, Err(Error::Busy(Some(31))));
    assert_eq!(state.retry_delay(), Some(Duration::from_secs(31)));
    state.retry_elapsed();
    assert_eq!(state.retry_delay(), None);
    assert_eq!(state.begin(), None);
}

#[test]
fn a_task_revision_honours_retry_after_with_no_retry_budget() {
    let mut state = state_after_five_retries();
    let flight = state.begin().unwrap();
    assert!(state.update(revised_task(), 0));
    state.finish(flight, Err(Error::Busy(Some(6))));
    assert_eq!(state.retry_delay(), Some(Duration::from_secs(6)));
    assert_eq!(state.begin(), None);
    state.retry_elapsed();
    assert_eq!(state.retry_delay(), None);
    let follow_up = state.begin().expect("read after Retry-After");
    state.finish(follow_up, Err(Error::Unreachable));
    assert_eq!(state.retry_delay(), None);
    assert_eq!(state.begin(), None);
}

#[test]
fn try_again_clears_a_hold_and_reads_at_once() {
    let (mut state, flight) = read_with_queued_task_revision();
    state.finish(flight, Err(Error::Busy(Some(31))));
    assert_eq!(state.begin(), None);
    assert!(state.can_retry());
    state.probe();
    assert_eq!(state.retry_delay(), None);
    assert!(state.begin().is_some());
}

#[test]
fn a_stale_assignment_read_still_holds_the_same_route_for_retry_after() {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let flight = state.begin().unwrap();
    assert!(state.update(roster("second", "Reviewing…", 2), 0));
    state.finish(flight, Err(Error::Busy(Some(31))));
    assert_eq!(state.begin(), None, "the new assignment must wait");
    assert_eq!(state.retry_delay(), Some(Duration::from_secs(31)));
    assert!(state.shown(true).1.is_none(), "the old error is not shown");
    assert_progress(&state, "Reviewing…");

    state.retry_elapsed();
    assert_eq!(state.retry_delay(), None);
    let follow_up = state
        .begin()
        .expect("read the new assignment after the hold");
    state.finish(follow_up, Err(Error::Unreachable));
    assert_eq!(state.begin(), None, "the hold released exactly one read");
    assert_eq!(
        state.retry_delay(),
        Some(Duration::from_secs(1)),
        "retry budget starts at zero"
    );
}

#[test]
fn a_reply_from_the_old_connection_does_not_hold_the_new_route() {
    let mut state = DetailState::new(roster("first", "Working…", 1), 0);
    let flight = state.begin().unwrap();
    assert!(state.update(roster("second", "Reviewing…", 2), 1));
    state.finish(flight, Err(Error::Busy(Some(31))));
    assert_eq!(state.retry_delay(), None);
    assert!(state.begin().is_some());
}
