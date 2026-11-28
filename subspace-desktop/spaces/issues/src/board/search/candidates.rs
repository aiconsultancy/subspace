//! Wider list choices preserve the current search and initiative.

use crate::model::{ProjectScope, SubIssues, Tab};
use crate::requests::ListChoice;

pub(super) fn search_candidates(
    scoped: &ListChoice,
    projects: Vec<ProjectScope>,
) -> Vec<ListChoice> {
    let without_filters = ListChoice {
        filters: crate::filters::Filters {
            archived: scoped.filters.archived,
            ..Default::default()
        },
        ..scoped.clone()
    };
    let all_tab = ListChoice {
        tab: Tab::All,
        ..scoped.clone()
    };
    let clear = ListChoice {
        tab: Tab::All,
        ..without_filters.clone()
    };
    let sub_issues = ListChoice {
        sub_issues: SubIssues::Nested,
        ..scoped.clone()
    };
    let nested = ListChoice {
        sub_issues: SubIssues::Nested,
        ..clear.clone()
    };
    let mut candidates = Vec::new();
    for wider in [
        all_tab,
        without_filters,
        sub_issues,
        clear.clone(),
        nested.clone(),
    ] {
        if &wider != scoped && !candidates.contains(&wider) {
            candidates.push(wider);
        }
    }
    for project in projects {
        candidates.push(ListChoice {
            project: project.clone(),
            ..clear.clone()
        });
        if clear.sub_issues != nested.sub_issues {
            candidates.push(ListChoice {
                project,
                ..nested.clone()
            });
        }
    }
    candidates
}
