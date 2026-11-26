//! Read demand and displayed text for same-revision task previews.

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
    let prefix = "🌊".repeat(399);
    let full = format!("{prefix}abcdefghijklmnopqrstuvwxyz: unique full tail");
    let projected = format!("{prefix}…");
    let roster = |progress: &str| {
        let mut entry = member("first", progress, 1);
        entry.contexts[0].current.as_mut().unwrap().updated_at = Some(42);
        entry
    };
    let feed = |progress| Roster::Present(Box::new(roster(progress)));
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
