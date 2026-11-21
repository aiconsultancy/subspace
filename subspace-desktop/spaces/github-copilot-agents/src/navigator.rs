//! What the navigator lists: the views and the repositories, each with the
//! number of rows it would show, and which of them is the current choice.
//! No GPUI and no requests.

use crate::filters::{self, Filters};
use crate::model::{AgentRow, Kind};
use crate::ui::ALL;

/// The views, in navigator order: every row, then each kind.
pub const VIEWS: [Option<Kind>; 3] = [None, Some(Kind::Cloud), Some(Kind::Local)];

/// What the view that keeps every kind is called.
pub const ALL_VIEW: &str = "All tasks and chats";

/// The name a view reads as, in the navigator and on the pane's tab.
pub fn view_label(view: Option<Kind>) -> &'static str {
    view.map_or(ALL_VIEW, Kind::label)
}

/// What a view stores as the kind filter: the kind's id, or the value that
/// stands for every kind.
pub fn view_value(view: Option<Kind>) -> &'static str {
    view.map_or(ALL, Kind::id)
}

/// A view's row id in the navigator.
pub fn view_key(view: Option<Kind>) -> String {
    format!(
        "github-copilot-agents-nav-view:{}",
        view.map_or("all", Kind::id)
    )
}

/// A repository's row id in the navigator.
pub fn repo_key(repo: &str) -> String {
    format!("github-copilot-agents-nav-repo:{repo}")
}

/// One navigator row: what it stands for, how many rows it would show, and
/// whether it is the current choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination<T> {
    pub value: T,
    pub count: usize,
    pub selected: bool,
}

/// How many of `rows` the filters `scope` keep.
fn kept(rows: &[AgentRow], scope: &Filters) -> usize {
    rows.iter().filter(|row| scope.keeps(row)).count()
}

/// The views, each with the rows it would show under the state and
/// repository filters in use. The kind filter is the view itself, so it does
/// not narrow the counts.
pub fn views(rows: &[AgentRow], filters: &Filters) -> Vec<Destination<Option<Kind>>> {
    VIEWS
        .into_iter()
        .map(|view| Destination {
            value: view,
            count: kept(
                rows,
                &Filters {
                    kind: view,
                    ..filters.clone()
                },
            ),
            selected: filters.kind == view,
        })
        .collect()
}

/// The repositories the rows name, each with the rows it would show under
/// the view and the state filter in use. A repository the current view
/// empties stays listed with a count of zero, so the list does not jump.
pub fn repositories(rows: &[AgentRow], filters: &Filters) -> Vec<Destination<String>> {
    filters::repos(rows)
        .into_iter()
        .map(|repo| Destination {
            count: kept(
                rows,
                &Filters {
                    repo: Some(repo.clone()),
                    ..filters.clone()
                },
            ),
            selected: filters
                .repo
                .as_deref()
                .is_some_and(|current| current.eq_ignore_ascii_case(&repo)),
            value: repo,
        })
        .collect()
}

/// What a click on a repository row stores as the repository filter: the
/// repository, or the value that clears the filter when the row is already
/// the choice, so a second click shows every repository again.
pub fn repo_choice(repo: &str, selected: bool) -> &str {
    if selected { ALL } else { repo }
}

#[cfg(test)]
mod tests {
    use super::{
        ALL_VIEW, Destination, VIEWS, repo_choice, repo_key, repositories, view_key, view_label,
        view_value, views,
    };
    use crate::filters::Filters;
    use crate::model::tests::row;
    use crate::model::{AgentRow, Kind, StateGroup};

    /// A running cloud task in `Owner/one`, a finished one in `Owner/two`
    /// and a saved local chat, which has no repository.
    fn rows() -> Vec<AgentRow> {
        let mut other = row("cloud:b", Kind::Cloud, "completed", "");
        other.repo = "Owner/two".into();
        let mut local = row("local:s", Kind::Local, "saved", "");
        local.repo = String::new();
        vec![row("cloud:a", Kind::Cloud, "in_progress", ""), other, local]
    }

    fn counts<T>(listed: &[Destination<T>]) -> Vec<usize> {
        listed.iter().map(|row| row.count).collect()
    }

    fn chosen<T>(listed: &[Destination<T>]) -> Vec<bool> {
        listed.iter().map(|row| row.selected).collect()
    }

    #[test]
    fn each_view_counts_the_rows_it_would_show() {
        let all = views(&rows(), &Filters::default());
        let values: Vec<Option<Kind>> = all.iter().map(|view| view.value).collect();
        assert_eq!(values, VIEWS);
        assert_eq!(counts(&all), [3, 2, 1]);
        assert_eq!(chosen(&all), [true, false, false]);

        let active = views(
            &rows(),
            &Filters {
                group: Some(StateGroup::Active),
                ..Filters::default()
            },
        );
        assert_eq!(counts(&active), [1, 1, 0], "the state filter narrows");

        let one_repo = views(
            &rows(),
            &Filters {
                repo: Some("owner/ONE".into()),
                ..Filters::default()
            },
        );
        assert_eq!(
            counts(&one_repo),
            [1, 1, 0],
            "a repository filter hides local chats, which have none"
        );
    }

    #[test]
    fn the_current_view_is_selected_and_does_not_narrow_the_other_views() {
        let cloud = views(
            &rows(),
            &Filters {
                kind: Some(Kind::Cloud),
                ..Filters::default()
            },
        );
        assert_eq!(chosen(&cloud), [false, true, false]);
        assert_eq!(counts(&cloud), [3, 2, 1]);
    }

    #[test]
    fn repositories_count_under_the_view_and_mark_the_choice() {
        let all = repositories(&rows(), &Filters::default());
        let names: Vec<&str> = all.iter().map(|repo| repo.value.as_str()).collect();
        assert_eq!(names, ["Owner/one", "Owner/two"]);
        assert_eq!(counts(&all), [1, 1]);
        assert_eq!(chosen(&all), [false, false]);

        let picked = repositories(
            &rows(),
            &Filters {
                repo: Some("owner/TWO".into()),
                ..Filters::default()
            },
        );
        assert_eq!(chosen(&picked), [false, true], "the match ignores case");
        assert_eq!(
            counts(&picked),
            [1, 1],
            "the repository filter does not empty the other repositories"
        );

        let local = repositories(
            &rows(),
            &Filters {
                kind: Some(Kind::Local),
                ..Filters::default()
            },
        );
        assert_eq!(counts(&local), [0, 0], "an emptied repository stays listed");
        assert!(repositories(&[], &Filters::default()).is_empty());
    }

    #[test]
    fn a_second_click_on_a_repository_clears_it() {
        assert_eq!(repo_choice("Owner/one", false), "Owner/one");
        assert_eq!(repo_choice("Owner/one", true), "");
    }

    #[test]
    fn views_read_as_their_kind_and_store_its_id() {
        assert_eq!(view_label(None), ALL_VIEW);
        assert_eq!(view_label(Some(Kind::Cloud)), Kind::Cloud.label());
        assert_eq!(view_value(None), "");
        assert_eq!(view_value(Some(Kind::Local)), "local");
        assert_eq!(
            Kind::from_id(view_value(Some(Kind::Cloud))),
            Some(Kind::Cloud)
        );
    }

    #[test]
    fn navigator_rows_have_ids_of_their_own() {
        assert_eq!(view_key(None), "github-copilot-agents-nav-view:all");
        assert_eq!(
            view_key(Some(Kind::Local)),
            "github-copilot-agents-nav-view:local"
        );
        assert_eq!(
            repo_key("Owner/one"),
            "github-copilot-agents-nav-repo:Owner/one"
        );
    }
}
