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
