use subspace_framework::lucide::IconName;

use super::{
    IssuePriority, ProjectScope, StateKind, SubIssues, Tab, chosen_project, default_project,
};
use crate::fixtures::project;

#[test]
fn tabs_keep_their_order_labels_and_stored_keys() {
    let labels: Vec<_> = Tab::ALL.iter().map(|tab| tab.spec().label).collect();
    assert_eq!(labels, ["My issues", "Active", "Backlog", "All issues"]);
    assert_eq!(Tab::Backlog.spec().icon, IconName::CircleDashed);
    assert_eq!(Tab::default(), Tab::Active);
    for tab in Tab::ALL {
        assert_eq!(Tab::from_key(tab.key()), Some(tab));
        assert_eq!(Tab::ALL[tab.index()], tab);
    }
    assert_eq!(Tab::from_key("cycles"), None);
}

#[test]
fn sub_issues_are_hidden_by_default_and_flat_when_on() {
    assert_eq!(SubIssues::default(), SubIssues::Hidden);
    assert_eq!(SubIssues::Hidden.toggled(), SubIssues::Flat);
    assert_eq!(SubIssues::Flat.toggled(), SubIssues::Hidden);
    assert_eq!(SubIssues::from_key("flat"), SubIssues::Flat);
    assert_eq!(SubIssues::from_key("nested"), SubIssues::Hidden);
}

#[test]
fn project_scope_uses_active_projects_and_preserves_a_saved_selection() {
    let projects = [
        project("SUB", "Subspace", false),
        project("OPS", "Operations", false),
    ];
    assert_eq!(default_project(&projects), ProjectScope::AllProjects);
    assert_eq!(
        default_project(&projects[..1]),
        ProjectScope::Project("SUB".into())
    );
    assert_eq!(
        chosen_project(Some(ProjectScope::Project("OPS".into())), &projects),
        ProjectScope::Project("OPS".into())
    );
    assert_eq!(
        chosen_project(Some(ProjectScope::Project("OPS".into())), &[]),
        ProjectScope::Project("OPS".into())
    );
    let archived = [
        project("SUB", "Subspace", false),
        project("OPS", "Operations", true),
    ];
    assert_eq!(
        chosen_project(Some(ProjectScope::Project("OPS".into())), &archived),
        ProjectScope::Project("SUB".into())
    );
    assert_eq!(ProjectScope::from_key("*"), ProjectScope::AllProjects);
}

#[test]
fn terminal_states_are_done_cancelled_and_duplicate() {
    let terminal: Vec<_> = [
        "triage",
        "backlog",
        "unstarted",
        "started",
        "completed",
        "canceled",
        "duplicate",
        "paused",
    ]
    .into_iter()
    .filter(|kind| StateKind::parse(kind).is_terminal())
    .collect();
    assert_eq!(terminal, ["completed", "canceled", "duplicate"]);
    assert_eq!(StateKind::parse("paused"), StateKind::Other);
}

#[test]
fn priorities_follow_api_values() {
    let labels: Vec<_> = (0..=5)
        .map(|value| IssuePriority::from_value(value).look().label)
        .collect();
    assert_eq!(
        labels,
        [
            "No priority",
            "Urgent",
            "High",
            "Medium",
            "Low",
            "No priority"
        ]
    );
}
