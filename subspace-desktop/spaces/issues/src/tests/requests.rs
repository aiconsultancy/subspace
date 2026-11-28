use subspace_api_client::{ListQuery, ListTab};

use super::{ListChoice, search_text};
use crate::model::{ProjectScope, SubIssues, Tab};

fn choice(tab: Tab, project: ProjectScope, search: &str, sub_issues: SubIssues) -> ListChoice {
    ListChoice {
        tab,
        project,
        search: search.into(),
        sub_issues,
        filters: Default::default(),
        initiative: None,
    }
}

#[test]
fn each_choice_becomes_the_hubs_list_query() {
    assert_eq!(
        choice(
            Tab::Backlog,
            ProjectScope::Project("SUB".into()),
            "fees",
            SubIssues::Hidden
        )
        .query(),
        ListQuery::new(ListTab::Backlog)
            .project(Some("SUB"))
            .search("fees")
            .show_subissues(false)
    );
    assert_eq!(
        choice(
            Tab::MyIssues,
            ProjectScope::AllProjects,
            "",
            SubIssues::Nested
        )
        .query(),
        ListQuery::new(ListTab::MyIssues)
            .project(None)
            .show_subissues(true),
        "All projects sends no project"
    );
    let tabs: Vec<_> = Tab::ALL.iter().map(|tab| tab.spec().list).collect();
    assert_eq!(
        tabs,
        [
            ListTab::MyIssues,
            ListTab::Active,
            ListTab::Backlog,
            ListTab::All
        ]
    );
}

#[test]
fn search_stops_at_two_hundred_utf16_units_on_a_character() {
    assert_eq!(search_text("  fee amendments  "), "fee amendments");
    assert_eq!(search_text(&"a".repeat(250)).len(), 200);
    let wide = "😀".repeat(101);
    let kept = search_text(&wide);
    assert_eq!(kept.chars().count(), 100, "each takes two units");
    assert_eq!(kept.encode_utf16().count(), 200);
}
