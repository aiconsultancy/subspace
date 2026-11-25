use serde_json::json;

use super::{DetailState, Roster, member};

#[test]
fn a_roster_update_keeps_pull_request_facts_together() {
    let mut roster = member("first", "Checking…", 1);
    roster.contexts[0].pull_request = Some(
        serde_json::from_value(json!({
            "number": 41, "url": "https://github.com/example/chart/pull/41",
            "title": "Chart the coast", "state": "draft", "checks": "running"
        }))
        .unwrap(),
    );
    let mut state = DetailState::new(Roster::Present(Box::new(roster.clone())), 0);
    let read = state.begin().unwrap();
    let mut full = roster.clone();
    let pr = full.contexts[0].pull_request.as_mut().unwrap();
    pr.number = 42;
    pr.url = "https://github.com/example/chart/pull/42".into();
    pr.title = "Chart the harbour".into();
    pr.checks = "none".into();
    state.finish(read, Ok(full));
    assert_eq!(
        state.shown(true).0.unwrap().contexts[0]
            .pull_request
            .as_ref()
            .unwrap()
            .number,
        42
    );

    let pr = roster.contexts[0].pull_request.as_mut().unwrap();
    pr.title = "Chart more of the coast".into();
    pr.state = "open".into();
    pr.checks = "passing".into();
    let expected = pr.clone();
    assert!(state.update(Roster::Present(Box::new(roster)), 0));
    assert_eq!(
        state.shown(true).0.unwrap().contexts[0]
            .pull_request
            .as_ref(),
        Some(&expected)
    );
}

#[test]
fn a_roster_update_keeps_git_checkout_facts_together() {
    let mut roster = member("first", "Checking…", 1);
    roster.contexts[0].git = Some(
        serde_json::from_value(json!({
            "repository": "example/chart", "branch": "chart-coast", "head": "123abcd",
            "base": "origin/main", "ahead": 1, "changedFiles": 0, "checkedAt": 1
        }))
        .unwrap(),
    );
    for field in ["repository", "branch", "head"] {
        let mut state = DetailState::new(Roster::Present(Box::new(roster.clone())), 0);
        let read = state.begin().unwrap();
        let mut full = roster.clone();
        let git = full.contexts[0].git.as_mut().unwrap();
        match field {
            "repository" => git.repository = Some("example/harbour".into()),
            "branch" => git.branch = Some("chart-coast-harbour".into()),
            "head" => git.head = Some("456abcd".into()),
            _ => unreachable!(),
        }
        git.base = Some("origin/stable".into());
        state.finish(read, Ok(full));
        assert_eq!(
            state.shown(true).0.unwrap().contexts[0]
                .git
                .as_ref()
                .unwrap()
                .base
                .as_deref(),
            Some("origin/stable")
        );

        let mut latest = roster.clone();
        let git = latest.contexts[0].git.as_mut().unwrap();
        git.ahead = Some(2);
        git.changed_files = Some(3);
        git.checked_at = 2;
        assert!(state.update(Roster::Present(Box::new(latest.clone())), 0));
        assert_eq!(
            state.shown(true).0.unwrap().contexts[0].git,
            latest.contexts[0].git,
            "{field}"
        );
    }
}
