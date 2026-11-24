use subspace_framework::dashboard::menu::MenuOption;

use super::{Entry, View, initiatives, projects, views};
use crate::model::{ProjectScope, Tab};

fn selected_views(current: View) -> Vec<&'static str> {
    views(current, None)
        .into_iter()
        .filter(|entry| entry.selected)
        .map(|entry| entry.view.label())
        .collect()
}

#[test]
fn the_views_keep_their_order_and_the_current_one_is_selected() {
    let labels: Vec<_> = views(View::Tab(Tab::Active), None)
        .into_iter()
        .map(|entry| entry.view.label())
        .collect();
    assert_eq!(
        labels,
        ["My issues", "Active", "Backlog", "All issues", "Activity"]
    );
    assert_eq!(selected_views(View::Tab(Tab::Active)), ["Active"]);
    assert_eq!(selected_views(View::Tab(Tab::All)), ["All issues"]);
    assert_eq!(selected_views(View::Activity), ["Activity"]);
}

#[test]
fn the_activity_feed_covers_the_chosen_tab() {
    assert_eq!(View::current(Tab::Backlog, false), View::Tab(Tab::Backlog));
    assert_eq!(View::current(Tab::Backlog, true), View::Activity);
}

#[test]
fn only_my_issues_carries_a_count() {
    let counted = views(View::Tab(Tab::Active), Some(3));
    let counts: Vec<_> = counted.iter().map(|entry| entry.count.as_deref()).collect();
    assert_eq!(counts, [Some("3"), None, None, None, None]);
    assert!(
        views(View::Tab(Tab::MyIssues), None)
            .iter()
            .all(|entry| entry.count.is_none()),
        "no count before it is read"
    );
    let zero = views(View::Activity, Some(0));
    assert_eq!(zero[0].count.as_deref(), Some("0"));
}

#[test]
fn a_view_reads_back_from_its_key() {
    for view in View::ALL {
        assert_eq!(View::from_key(view.key()), Some(view));
    }
    assert_eq!(View::Activity.key(), "activity");
    assert_eq!(View::Tab(Tab::MyIssues).key(), "my-issues");
    assert_eq!(View::from_key("elsewhere"), None);
}

#[test]
fn every_project_leads_the_projects_and_the_current_scope_is_selected() {
    let options = || {
        [
            (
                ProjectScope::Project("SUB".into()),
                "SUB · Subspace".to_owned(),
            ),
            (
                ProjectScope::Project("OPS".into()),
                "OPS · Operations".to_owned(),
            ),
            (ProjectScope::AllProjects, "All projects".to_owned()),
        ]
        .into_iter()
    };
    let rows = projects(options(), &ProjectScope::Project("OPS".into()));
    assert_eq!(
        rows,
        [
            entry("*", "All projects", false),
            entry("SUB", "SUB · Subspace", false),
            entry("OPS", "OPS · Operations", true),
        ]
    );
    let all = projects(options(), &ProjectScope::AllProjects);
    assert_eq!(all[0], entry("*", "All projects", true));
    assert!(all[1..].iter().all(|row| !row.selected));
}

#[test]
fn an_initiative_without_every_project_lists_only_its_projects() {
    let scoped = [(
        ProjectScope::Project("SUB".into()),
        "SUB · Subspace".to_owned(),
    )];
    assert_eq!(
        projects(scoped.into_iter(), &ProjectScope::Project("SUB".into())),
        [entry("SUB", "SUB · Subspace", true)]
    );
    assert!(projects(std::iter::empty(), &ProjectScope::AllProjects).is_empty());
}

#[test]
fn initiatives_list_the_pickers_options_once_there_is_a_choice() {
    let options = [
        MenuOption::new("ops", "Operations").checked(true),
        MenuOption::new("design", "Design"),
    ];
    assert_eq!(
        initiatives(&options),
        [
            entry("ops", "Operations", true),
            entry("design", "Design", false),
        ]
    );
    assert!(
        initiatives(&options[1..]).is_empty(),
        "one membership: nothing to choose"
    );
}

fn entry(value: &str, label: &str, selected: bool) -> Entry {
    Entry {
        value: value.into(),
        label: label.into(),
        selected,
    }
}
