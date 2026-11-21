//! The repository filter: the list shows one repository's pull requests, or
//! every repository of the view. It lives in memory only and clears when the
//! account changes.
//!
//! The navigator lists the view's repositories under "Repositories"; a click
//! filters to one and a second click clears it. While the navigator is hidden
//! the view menu lists them too, and the toolbar shows a clear button while a
//! filter is set.

use gpui_kit::{Context, IntoElement as _, SharedString};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::list::Group;
use subspace_framework::dashboard::menu::MenuOption;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::repositories::is_favourite;
use subspace_framework::dashboard::workspace::NavRow;

use crate::Github;
use crate::model::{RepoGroup, group_by_repo, group_key};

/// A view menu value that names a repository, then `owner/name`.
pub(crate) const MENU_PREFIX: &str = "repo-filter:";

/// A repository with rows in the current view, before the filter applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoEntry {
    /// `owner/name`.
    pub(crate) name: String,
    pub(crate) count: usize,
}

/// Whether `repo` is the filtered repository.
pub(crate) fn is_filtered(filter: Option<&str>, repo: &str) -> bool {
    filter.is_some_and(|filter| filter.eq_ignore_ascii_case(repo))
}

/// The filter after a pick of `repo`: that repository, or none when it was
/// already the filter.
pub(crate) fn toggled(filter: Option<&str>, repo: &str) -> Option<String> {
    match is_filtered(filter, repo) {
        true => None,
        false => Some(repo.to_owned()),
    }
}

/// The list's groups with the filter applied: only the filtered
/// repository's group, or all of them when nothing is filtered.
pub(crate) fn apply(groups: Vec<Group>, filter: Option<&str>) -> Vec<Group> {
    let Some(filter) = filter else {
        return groups;
    };
    let key = group_key(filter);
    groups
        .into_iter()
        .filter(|group| group.key.eq_ignore_ascii_case(&key))
        .collect()
}

/// The view's repositories with their counts, in the list's order. With
/// Favourites only on (`favourites` is `Some`), only the favourites.
pub(crate) fn entries(groups: &[RepoGroup<'_>], favourites: Option<&[String]>) -> Vec<RepoEntry> {
    groups
        .iter()
        .filter(|group| favourites.is_none_or(|list| is_favourite(list, group.repo)))
        .map(|group| RepoEntry {
            name: group.repo.to_owned(),
            count: group.items.len(),
        })
        .collect()
}

/// The status line with the filter named at its end.
pub(crate) fn with_note(line: String, filter: Option<&str>) -> String {
    match filter {
        Some(repo) => format!("{line} · {repo} only"),
        None => line,
    }
}

impl Github {
    /// The view's repositories, before the filter applies.
    pub(crate) fn repo_entries(&self) -> Vec<RepoEntry> {
        let favourites = self
            .favourites
            .only
            .then_some(self.favourites.list.as_slice());
        entries(&group_by_repo(self.section_items()), favourites)
    }

    /// Filters the list to `repo`, or clears the filter when it is `repo`.
    pub(crate) fn pick_repository(&mut self, repo: &str, cx: &mut Context<Self>) {
        self.repo_filter = toggled(self.repo_filter.as_deref(), repo);
        self.rebuild(cx);
    }

    /// Shows every repository of the view again.
    pub(crate) fn clear_repository_filter(&mut self, cx: &mut Context<Self>) {
        if self.repo_filter.take().is_some() {
            self.rebuild(cx);
        }
    }

    /// One navigator row per repository; the filtered one is selected.
    pub(crate) fn repository_rows(&self, cx: &Context<Self>) -> Vec<NavRow> {
        self.repo_entries()
            .into_iter()
            .map(|entry| {
                let selected = is_filtered(self.repo_filter.as_deref(), &entry.name);
                let name = entry.name.clone();
                NavRow::new(format!("gh-repo:{}", entry.name), entry.name)
                    .icon(IconName::BookMarked)
                    .count(entry.count.to_string())
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_repository(&name, cx);
                    }))
            })
            .collect()
    }

    /// The view menu's repository options, after the views.
    pub(crate) fn repository_options(&self) -> Vec<MenuOption> {
        self.repo_entries()
            .into_iter()
            .map(|entry| {
                let checked = is_filtered(self.repo_filter.as_deref(), &entry.name);
                let value = format!("{MENU_PREFIX}{}", entry.name);
                let label = format!("Only {} · {} items", entry.name, entry.count);
                MenuOption::new(value, label).checked(checked)
            })
            .collect()
    }

    /// The toolbar's clear button, while a repository is filtered.
    pub(crate) fn repository_filter_button(
        &self,
        cx: &Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        let repo = self.repo_filter.as_deref()?;
        let label = SharedString::from(format!("{repo} only"));
        let button = action_button(
            "gh-repo-filter-clear",
            Intent::Neutral,
            Some(IconName::X),
            label,
        )
        .accessibility_label(format!("Show all repositories, not only {repo}"))
        .on_click(cx.listener(|this, _, _, cx| this.clear_repository_filter(cx)));
        Some(button.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use subspace_framework::client_github::model::ItemKind;
    use subspace_framework::dashboard::Dashboard as _;
    use subspace_framework::dashboard::list::Group;

    use super::{apply, entries, is_filtered, toggled, with_note};
    use crate::model::group_by_repo;
    use crate::{Github, SectionData, rows, sample};

    fn groups() -> Vec<Group> {
        let mut items = sample::items();
        items.retain(|item| item.kind == ItemKind::Pr);
        rows::groups(&items, "someone", i64::MAX / 2)
    }

    fn keys(groups: &[Group]) -> Vec<&str> {
        groups.iter().map(|group| group.key.as_ref()).collect()
    }

    #[test]
    fn no_filter_keeps_every_repository() {
        let all = apply(groups(), None);
        let expected = [
            "repo:octo-org/api",
            "repo:octo-org/web",
            "repo:octocat/subspace",
        ];
        assert_eq!(keys(&all), expected);
    }

    #[test]
    fn a_filter_keeps_only_its_repository() {
        let one = apply(groups(), Some("octo-org/web"));
        assert_eq!(keys(&one), ["repo:octo-org/web"]);
        assert_eq!(one[0].rows.len(), 2);
        let any_case = apply(groups(), Some("Octo-Org/Web"));
        assert_eq!(keys(&any_case), ["repo:octo-org/web"]);
    }

    #[test]
    fn a_filter_whose_repository_has_no_rows_leaves_the_list_empty() {
        assert!(apply(groups(), Some("example/elsewhere")).is_empty());
    }

    #[test]
    fn a_second_pick_of_the_filtered_repository_clears_it() {
        assert_eq!(
            toggled(None, "octo-org/api").as_deref(),
            Some("octo-org/api")
        );
        let other = toggled(Some("octo-org/api"), "octo-org/web");
        assert_eq!(other.as_deref(), Some("octo-org/web"));
        assert_eq!(toggled(Some("octo-org/api"), "octo-org/api"), None);
        assert!(is_filtered(Some("octo-org/api"), "Octo-Org/API"));
        assert!(!is_filtered(None, "octo-org/api"));
    }

    #[test]
    fn the_entries_follow_the_list_and_favourites_only() {
        let mut items = sample::items();
        items.retain(|item| item.kind == ItemKind::Pr);
        let groups = group_by_repo(&items);
        let all = entries(&groups, None);
        let names: Vec<&str> = all.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["octo-org/api", "octo-org/web", "octocat/subspace"]);
        let counts: Vec<usize> = all.iter().map(|entry| entry.count).collect();
        assert_eq!(counts, [2, 2, 1]);
        let favourites = vec!["octo-org/web".to_owned()];
        let only = entries(&groups, Some(&favourites));
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name, "octo-org/web");
        assert!(entries(&groups, Some(&[])).is_empty());
    }

    #[test]
    fn the_status_line_names_the_filtered_repository() {
        let line = "GitHub · Signed in as octo-dev".to_owned();
        assert_eq!(with_note(line.clone(), None), line);
        let noted = with_note(line, Some("octo-org/api"));
        assert_eq!(noted, "GitHub · Signed in as octo-dev · octo-org/api only");
    }

    #[gpui_kit::test]
    fn the_status_counts_stay_the_views_while_a_repository_is_filtered(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            subspace_framework::dashboard::prefs::init_file(None, cx);
        });
        let window = cx.add_window(Github::new);
        window
            .update(cx, |board, _, cx| {
                let mut items = sample::items();
                items.retain(|item| item.kind == ItemKind::Pr);
                assert_eq!(items.len(), 5);
                board.account = Some("octo-dev".into());
                let section = board.section;
                let account = board.data.entry("octo-dev".into()).or_default();
                let data = SectionData {
                    total: 5,
                    items,
                    at: 0,
                };
                account.sections.insert(section, data);
                board.rebuild(cx);
                let whole = Some("5 open PRs · 3 repos");
                assert_eq!(board.view_counts().as_deref(), whole);
                board.pick_repository("octo-org/web", cx);
                assert_eq!(board.list.groups().len(), 1);
                assert_eq!(board.view_counts().as_deref(), whole);
            })
            .unwrap();
    }
}
