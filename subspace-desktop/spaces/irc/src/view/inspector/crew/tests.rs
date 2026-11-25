use serde_json::json;
use subspace_client_fleet::Error;
use subspace_client_fleet::model::{CrewMember, ShipDetail};

use super::{DetailState, Roster};

fn member(id: &str, progress: &str, checked: u64) -> CrewMember {
    serde_json::from_value(json!({"id":"wren", "name":"Wren", "contexts":[{
        "project":"chart", "current":{"id":id,"summary":"Chart the coast","progress":progress},
        "git":{"checkedAt":checked}
    }]}))
    .unwrap()
}

fn roster(id: &str, progress: &str, checked: u64) -> Roster {
    Roster::Present(Box::new(member(id, progress, checked)))
}

fn assert_progress(state: &DetailState, progress: &str) {
    let shown = state.shown(true).0.unwrap();
    assert_eq!(state.current_task(&shown).unwrap().progress, progress);
    assert_eq!(
        shown.contexts[0].current.as_ref().unwrap().progress,
        progress
    );
}

#[test]
fn changes_during_the_first_read_coalesce_and_only_the_latest_result_is_used() {
    let mut state = DetailState::new(roster("first", "Starting…", 1), 0);
    let first = state.begin().unwrap();
    assert!(state.update(roster("second", "Checking…", 2), 0));
    assert!(state.update(roster("third", "Reviewing…", 3), 0));
    assert_eq!(state.begin(), None, "one read in flight");
    state.finish(first, Ok(member("first", "Obsolete response", 1)));
    assert_progress(&state, "Reviewing…");
    let latest = state.begin().unwrap();
    state.finish(first, Ok(member("first", "Late response", 1)));
    assert_progress(&state, "Reviewing…");
    let full = "Reviewing the soundings. ".repeat(180);
    state.finish(latest, Ok(member("third", &full, 3)));
    assert_progress(&state, &full);
    assert_eq!(state.begin(), None, "only one follow-up is needed");
}

#[test]
fn task_changes_after_a_read_show_the_roster_task_until_the_new_full_detail() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    let previous = "Checking the navigation marks. ".repeat(160);
    state.finish(first, Ok(member("first", &previous, 1)));
    assert_progress(&state, &previous);
    // Even a new task with the same bounded text should get the roster fallback.
    assert!(state.update(roster("second", "Checking…", 2), 0));
    assert_progress(&state, "Checking…");
    let latest = state.begin().unwrap();
    let full = "Checking the new chart. ".repeat(180);
    state.finish(latest, Ok(member("second", &full, 2)));
    assert_progress(&state, &full);
}

#[test]
fn git_only_changes_trigger_a_read_but_keep_full_text_in_the_meantime() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    let previous = "Checking the navigation marks. ".repeat(160);
    state.finish(first, Ok(member("first", &previous, 1)));
    assert!(!state.update(roster("first", "Checking…", 1), 0));
    assert_eq!(state.begin(), None);
    assert!(state.update(roster("first", "Checking…", 2), 0));
    let latest = state.begin().unwrap();
    assert_progress(&state, &previous);
    let full = "Checking the rest of the navigation marks. ".repeat(160);
    state.finish(latest, Ok(member("first", &full, 2)));
    assert_progress(&state, &full);
}

#[test]
fn summary_phase_and_progress_changes_each_replace_the_bounded_task_temporarily() {
    for change in ["summary", "phase", "progress"] {
        let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
        let first = state.begin().unwrap();
        state.finish(first, Ok(member("first", "Full chart progress", 1)));
        let mut live = member("first", "Checking…", 1);
        let task = live.contexts[0].current.as_mut().unwrap();
        match change {
            "summary" => task.summary = "A new summary".into(),
            "phase" => task.phase = "review".into(),
            _ => task.progress = "More work".into(),
        }
        let expected = task.clone();
        assert!(state.update(Roster::Present(Box::new(live)), 0));
        let shown = state.shown(true).0.unwrap();
        assert_eq!(state.current_task(&shown), Some(&expected), "{change}");
        assert_eq!(
            shown.contexts[0].current.as_ref(),
            Some(&expected),
            "{change}"
        );
    }
}

#[test]
fn directory_revision_invalidates_a_held_detail_and_an_in_flight_read() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    state.finish(
        first,
        Ok(member("first", "Full text from the old address", 1)),
    );
    assert!(state.update(roster("first", "Checking…", 2), 0));
    let old = state.begin().unwrap();
    assert!(state.update(roster("first", "Checking…", 2), 1));
    assert_progress(&state, "Checking…");
    assert_eq!(state.begin(), None);
    state.finish(old, Ok(member("first", "Obsolete address", 2)));
    assert_progress(&state, "Checking…");
    let new = state.begin().unwrap();
    state.finish(
        new,
        Ok(member("first", "Full text from the new address", 2)),
    );
    assert_progress(&state, "Full text from the new address");
}

#[test]
fn removal_and_unavailable_roster_win_after_a_change_even_if_a_read_succeeds() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    state.finish(first, Ok(member("first", "Full chart progress", 1)));
    assert!(state.update(roster("first", "Checking…", 2), 0));
    assert_progress(&state, "Full chart progress");
    let pending = state.begin().unwrap();
    assert!(state.shown(false).0.is_none());
    assert!(state.update(Roster::Unavailable, 0));
    assert!(state.shown(true).1.unwrap().contains("unavailable"));
    assert!(state.update(Roster::Gone, 0));
    state.finish(pending, Ok(member("first", "Late chart progress", 2)));
    assert!(state.shown(true).0.is_none());
    assert!(state.shown(true).1.unwrap().contains("no longer"));
    assert_eq!(state.begin(), None);
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
        Some("Crew detail is unavailable. Showing the ship's roster data.")
    );
}

#[test]
fn refresh_not_found_switches_from_full_detail_to_the_roster() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    state.finish(first, Ok(member("first", "Full chart progress", 1)));
    assert!(state.update(roster("first", "Checking…", 2), 0));
    assert_progress(&state, "Full chart progress");
    let refresh = state.begin().unwrap();
    state.finish(refresh, Err(Error::NotFound));
    assert_progress(&state, "Checking…");
    assert_eq!(
        state.shown(true).1,
        Some("Crew detail is unavailable. Showing the ship's roster data.")
    );
}
