//! Read demand and displayed text for same-revision task previews.

use subspace_client_fleet::Error;
use subspace_client_fleet::model::CrewMember;

use super::super::state::Field;
use super::{DetailState, Roster, assert_progress, member, roster};

#[test]
fn divergent_summary_phase_and_progress_previews_each_queue_a_reread() {
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
        let full = member("first", "Full chart progress", 1);
        assert!(state.update(Roster::Present(Box::new(live)), 0));
        let shown = state.shown(true).0.unwrap();
        assert_eq!(shown.current_task(), full.current_task(), "{change}");
        assert!(state.begin().is_some(), "{change} changed");
    }
}

#[test]
fn same_revision_previews_keep_full_text_and_only_divergence_rearms() {
    let full = format!("unique full head: {}unchanged", "🌊".repeat(2000));
    let projected = progress_tail(&full);
    let roster = |progress| revision(progress, 42);
    let feed = |progress| present(roster(progress));
    let mut state = DetailState::new(feed("Starting…"), 0);
    let first = state.begin().unwrap();
    state.finish(first, Ok(roster(&full)));
    assert!(state.update(feed(&projected), 0));
    assert_progress(&state, &full);
    assert_eq!(state.begin(), None, "matching preview does not poll");

    assert!(state.update(feed("Changing course…"), 0));
    assert!(state.update(feed("Sailing north…"), 0));
    assert_progress(&state, &full);
    let follow_up = state.begin().expect("divergence requeues");
    assert_eq!(state.begin(), None, "one read at a time");
    let next = "Sailing north beyond the markers. ".repeat(100);
    state.finish(follow_up, Ok(roster(&next)));
    assert_progress(&state, &next);
    assert!(!state.update(feed("Sailing north…"), 0));
    assert_eq!(state.begin(), None, "replaying the preview does not poll");
}

#[test]
fn a_new_revision_with_the_same_tail_does_not_shorten_fetched_text() {
    let full = format!("unique full head: {}unchanged", "🌊".repeat(2000));
    let mut state = DetailState::new(present(revision("Starting", 1)), 0);
    let first = state.begin().unwrap();
    state.finish(first, Ok(revision(&full, 1)));
    assert!(state.update(present(revision(progress_tail(&full), 2)), 0));
    assert_progress(&state, &full);
    assert!(
        state.begin().is_some(),
        "a new revision still queues a read"
    );
}

#[test]
fn a_new_tail_is_partial_until_the_full_revision_arrives_even_after_a_failure() {
    let full = "🌊".repeat(2000);
    let next = format!("{full} Finished the sounding.");
    let mut state = DetailState::new(present(revision(progress_tail(&full), 1)), 0);
    let marked = |state: &DetailState| {
        state
            .shown(true)
            .2
            .fields(Some("chart"))
            .contains(Field::Progress)
    };
    assert!(
        marked(&state),
        "initial roster already contains a capped tail"
    );
    let first = state.begin().unwrap();
    state.finish(first, Ok(revision(&full, 1)));
    assert!(!marked(&state));

    assert!(state.update(present(revision(progress_tail(&next), 2)), 0));
    assert_progress(&state, progress_tail(&next));
    assert!(marked(&state), "no splicing or unmarked replacement");
    let read = state.begin().unwrap();
    state.finish(read, Err(Error::Unreachable));
    assert!(state.shown(true).1.unwrap().contains("last available"));
    assert!(marked(&state));
    assert_progress(&state, progress_tail(&next));
    state.probe();
    let read = state.begin().unwrap();
    state.finish(read, Ok(revision(&next, 2)));
    assert_progress(&state, &next);
    assert!(!marked(&state));
    assert!(state.shown(true).1.is_none());
}

#[test]
fn capped_heads_do_not_replace_or_rearm_fetched_summary_and_phase() {
    let (summary, phase) = ("🌊".repeat(180), "λ".repeat(80));
    let mut state = DetailState::new(present(revision("Starting", 1)), 0);
    let first = state.begin().unwrap();
    let mut detail = revision("Full progress", 1);
    let task = detail.contexts[0].current.as_mut().unwrap();
    task.summary = format!("{summary} more summary");
    task.phase = format!("{phase} more phase");
    state.finish(first, Ok(detail.clone()));

    let mut bounded = revision("Starting", 1);
    let task = bounded.contexts[0].current.as_mut().unwrap();
    task.summary = summary;
    task.phase = phase;
    assert!(state.update(present(bounded), 0));
    assert_eq!(
        state.shown(true).0.unwrap().current_task(),
        detail.current_task()
    );
    assert_eq!(state.begin(), None);
}

fn revision(progress: &str, updated: i64) -> CrewMember {
    let mut entry = member("first", progress, 1);
    entry.contexts[0].current.as_mut().unwrap().updated_at = Some(updated);
    entry
}

fn present(member: CrewMember) -> Roster {
    Roster::Present(Box::new(member))
}

fn progress_tail(text: &str) -> &str {
    let offset = text
        .char_indices()
        .rev()
        .nth(399)
        .map_or(0, |(offset, _)| offset);
    &text[offset..]
}
