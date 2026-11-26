use super::{CrewContext, Project, pr_button_name, project_key};

#[test]
fn pr_button_names_distinguish_numbers_and_projects() {
    assert_eq!(pr_button_name(123, "charts"), "Open PR #123 for charts");
    assert_ne!(pr_button_name(123, "charts"), pr_button_name(124, "charts"));
    assert_ne!(pr_button_name(123, "charts"), pr_button_name(123, "radar"));
}

#[test]
fn project_keys_keep_default_rows_distinct_from_named_rows_and_each_other() {
    assert_ne!(project_key(None, 0), project_key(Some(""), 1));
    assert_ne!(project_key(None, 0), project_key(Some("default:0"), 0));
    assert_eq!(
        project_key(Some("charts"), 0),
        project_key(Some("charts"), 1)
    );
}

#[test]
fn project_keeps_git_counts_and_pull_request_details_outside_the_text_pager() {
    let summary = "Chart the coast and mark safe approaches. ".repeat(80);
    let progress = "Tests passed; reviewing the final changes. ".repeat(80);
    let context: CrewContext = serde_json::from_value(serde_json::json!({
        "project": "charts", "activity": "running",
        "current": {"summary": &summary, "phase": "Testing", "progress": &progress},
        "git": {"branch": "feat/coast", "head": "abcdef0", "base": "origin/main", "ahead": 2, "behind": 0, "changedFiles": 5},
        "pullRequest": {"number": 7, "title": "Chart the coast", "url": "https://example.com/maps/charts/pull/7", "state": "draft", "checks": "running"}
    })).unwrap();
    let project = Project::of(&context);
    assert_eq!(project.name, "charts");
    for (label, expected) in [
        ("Branch", "feat/coast"),
        ("Base", "origin/main"),
        ("Ahead", "2"),
        ("Behind", "0"),
        ("Changed paths", "5"),
        ("Pull request", "#7 Chart the coast"),
        ("PR state", "draft"),
        ("Checks", "running"),
    ] {
        assert!(
            project
                .facts
                .iter()
                .any(|(key, value)| *key == label && value == expected),
            "{label}"
        );
    }
    assert!(
        !project
            .facts
            .iter()
            .any(|(label, _)| matches!(*label, "Current task" | "Phase" | "Progress"))
    );
    assert!(
        project.pull_request.is_none(),
        "no dead button for an unsupported PR URL"
    );
}

#[test]
fn project_distinguishes_no_checkout_from_an_unavailable_git_read() {
    for (json, expected) in [
        (
            r#"{"provisioned":"pending","git":null}"#,
            "No checkout yet.",
        ),
        (
            r#"{"provisioned":"ready","git":{"error":"Git state unavailable."}}"#,
            "Git state unavailable.",
        ),
        (r#"{"git":null}"#, "Git state unavailable."),
    ] {
        let context = serde_json::from_str(json).unwrap();
        let project = Project::of(&context);
        assert_eq!(project.name, "Default project");
        assert!(project.facts.contains(&("Git", expected.to_owned())));
        assert!(!project.facts.iter().any(|(key, _)| *key == "Ahead"));
        assert!(project.pull_request.is_none());
    }
}
#[test]
fn control_codes_are_visible_without_reordering_or_cutting_the_text() {
    let text = "A\tB\r\nC\u{202e}D\x1bE\u{0085}F".repeat(500);
    let expected = "A\tB\r\nC⟨U+202E⟩D⟨U+001B⟩E⟨U+0085⟩F".repeat(500);
    assert_eq!(super::visible(&text), expected);
}
