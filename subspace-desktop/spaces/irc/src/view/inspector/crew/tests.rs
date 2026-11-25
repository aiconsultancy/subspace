use serde_json::json;
use subspace_client_fleet::Error;
use subspace_client_fleet::model::CrewMember;

use super::{DetailState, Roster};

mod recovery;

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
    assert_eq!(shown.current_task().unwrap().progress, progress);
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
fn git_only_changes_keep_full_text_without_triggering_another_read() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    let previous = "Checking the navigation marks. ".repeat(160);
    state.finish(first, Ok(member("first", &previous, 1)));
    assert!(!state.update(roster("first", "Checking…", 1), 0));
    assert_eq!(state.begin(), None);
    assert!(state.update(roster("first", "Checking…", 2), 0));
    assert_eq!(state.begin(), None);
    assert_progress(&state, &previous);
}

#[test]
fn checked_at_churn_does_not_discard_a_read_or_queue_a_follow_up() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    for checked in 2..=4 {
        assert!(state.update(roster("first", "Checking…", checked), 0));
        assert_eq!(state.begin(), None);
    }
    let full = "Full first response. ".repeat(200);
    state.finish(first, Ok(member("first", &full, 1)));
    assert_progress(&state, &full);
    assert_eq!(state.begin(), None);
    let revised = |checked| {
        let mut member = member("first", "Checking again…", checked);
        member.contexts[0].current.as_mut().unwrap().updated_at = Some(5);
        Roster::Present(Box::new(member))
    };
    assert!(state.update(revised(5), 0));
    assert!(state.has_full_detail());
    assert_progress(&state, "Checking again…");
    let follow_up = state.begin().unwrap();
    assert!(state.update(revised(6), 0));
    assert_eq!(state.begin(), None);
    let newer = "Full follow-up response. ".repeat(200);
    state.finish(follow_up, Ok(member("first", &newer, 5)));
    assert_progress(&state, &newer);
    assert_eq!(state.begin(), None);
}

#[test]
fn tail_only_progress_changes_refresh_full_detail() {
    let prefix = "a".repeat(400);
    let revision = |progress: &str, at| {
        let mut crew = member("first", progress, 1);
        crew.contexts[0].current.as_mut().unwrap().updated_at = Some(at);
        crew
    };
    let bounded = |at| Roster::Present(Box::new(revision(&prefix, at)));
    let mut state = DetailState::new(bounded(1), 0);
    let first = state.begin().unwrap();
    let old = format!("{prefix} old tail");
    let new = format!("{prefix} new tail");
    state.finish(first, Ok(revision(&old, 1)));
    assert_progress(&state, &old);
    assert!(state.update(bounded(2), 0));
    let read = state
        .begin()
        .expect("updatedAt triggers a read despite identical roster text");
    state.finish(read, Ok(revision(&new, 2)));
    assert_progress(&state, &new);

    let mut crew = revision("Different roster text alone", 2);
    assert!(state.update(Roster::Present(Box::new(crew.clone())), 0));
    assert_eq!(state.begin(), None);
    crew.contexts[0].current.as_mut().unwrap().status =
        subspace_client_fleet::model::TaskStatus::Running;
    assert!(state.update(Roster::Present(Box::new(crew)), 0));
    assert!(state.begin().is_some(), "status alone triggers a read");
}

#[test]
fn a_response_merges_task_text_changed_during_the_read() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    assert!(state.update(roster("first", "Checking the next chart…", 2), 0));
    state.finish(first, Ok(member("first", "Full older progress", 1)));
    assert!(state.has_full_detail());
    assert_progress(&state, "Checking the next chart…");
}

#[test]
fn a_routine_git_change_keeps_unchanged_full_fields() {
    let mut bounded = member("first", "Checking…", 1);
    bounded.contexts[0].git.as_mut().unwrap().branch = Some("Short…".into());
    let mut state = DetailState::new(Roster::Present(Box::new(bounded.clone())), 0);
    let read = state.begin().unwrap();
    let mut full = bounded.clone();
    full.contexts[0].git.as_mut().unwrap().branch = Some("The complete branch name".into());
    let git = bounded.contexts[0].git.as_mut().unwrap();
    git.ahead = Some(2);
    git.checked_at = 3;
    assert!(state.update(Roster::Present(Box::new(bounded)), 0));
    state.finish(read, Ok(full));
    let shown = state.shown(true).0.unwrap();
    let git = shown.contexts[0].git.as_ref().unwrap();
    assert_eq!(git.branch.as_deref(), Some("The complete branch name"));
    assert_eq!(git.ahead, Some(2));
    assert_eq!(state.begin(), None);
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
        let mut expected = task.clone();
        if change != "progress" {
            expected.progress = "Full chart progress".into();
        }
        assert!(state.update(Roster::Present(Box::new(live)), 0));
        let shown = state.shown(true).0.unwrap();
        assert_eq!(shown.current_task(), Some(&expected), "{change}");
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
    state.rearm();
    let old = state.begin().unwrap();
    assert!(state.update(roster("first", "Checking…", 2), 1));
    assert_progress(&state, "Checking…");
    let new = state
        .begin()
        .expect("new directory does not wait for old I/O");
    state.finish(old, Ok(member("first", "Obsolete address", 2)));
    assert_progress(&state, "Checking…");
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
    state.rearm();
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
fn a_transient_failure_can_be_retried_without_roster_changes() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    state.finish(first, Err(Error::Busy(Some(6))));
    assert!(state.can_retry());
    assert!(state.shown(true).1.unwrap().contains("Could not load"));
    assert_eq!(state.begin(), None);
    assert_eq!(state.retry_delay(), Some(std::time::Duration::from_secs(6)));
    state.retry_elapsed();
    let retry = state.begin().unwrap();
    assert!(!state.can_retry());
    assert_eq!(state.begin(), None);
    state.finish(retry, Ok(member("first", "Full detail after retry", 1)));
    assert_progress(&state, "Full detail after retry");
    assert!(state.shown(true).1.is_none());
}

#[test]
fn retry_backoff_is_bounded_and_does_not_poll_early() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    for seconds in [1, 2, 4, 8, 16] {
        let flight = state.begin().unwrap();
        state.finish(flight, Err(Error::Unreachable));
        assert_eq!(
            state.retry_delay(),
            Some(std::time::Duration::from_secs(seconds))
        );
        state.retry_elapsed();
    }
    let flight = state.begin().unwrap();
    state.finish(flight, Err(Error::Unreachable));
    assert_eq!(state.retry_delay(), None);
    assert!(state.can_retry());
    state.probe();
    let manual = state.begin().unwrap();
    state.finish(manual, Err(Error::Busy(Some(31))));
    assert_eq!(
        state.retry_delay(),
        Some(std::time::Duration::from_secs(31))
    );
    state.retry_elapsed();
    assert_eq!(state.begin(), None, "a long wait is not an automatic retry");
}

#[test]
fn refresh_not_found_switches_from_full_detail_to_the_roster() {
    let mut state = DetailState::new(roster("first", "Checking…", 1), 0);
    let first = state.begin().unwrap();
    state.finish(first, Ok(member("first", "Full chart progress", 1)));
    state.rearm();
    assert_progress(&state, "Full chart progress");
    let refresh = state.begin().unwrap();
    state.finish(refresh, Err(Error::NotFound));
    assert_progress(&state, "Checking…");
    assert_eq!(
        state.shown(true).1,
        Some(
            "Crew detail is unavailable. Showing the ship's roster data. Older Stargate gateways don't serve crew detail."
        )
    );
}
