//! The issue-list search is scoped to the active board and debounced.
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{Context, Entity, Task};

use subspace_api_client::hub::WiderSearch;
use subspace_framework::contracts::issues::recognition::ReferenceRecognizer;
use subspace_framework::dashboard::prefs;

use super::{FOLDED_PREF, PROJECT_PREF, SUB_ISSUES_PREF, TAB_PREF, UNFOLDED_PREF};
use crate::Issues;
use crate::listing::{IssueGroup, More, Status};
use crate::model::{ProjectScope, StateKind};
use crate::requests::{ListChoice, search_text};

mod candidates;

/// Typing waits this long before the list is read again.
const SEARCH_PAUSE: Duration = Duration::from_millis(300);

#[derive(Default)]
pub(crate) enum SearchFallback {
    #[default]
    Idle,
    Typing,
    Checked,
    Checking {
        _task: Task<()>,
    },
    Hidden(Box<HiddenMatch>),
}

pub(crate) struct HiddenMatch {
    scoped: ListChoice,
    wider: ListChoice,
    found: WiderSearch,
}

impl HiddenMatch {
    pub(crate) fn action(&self) -> String {
        if self.scoped.project != self.wider.project {
            return match &self.wider.project {
                ProjectScope::Project(key) => format!("Search {key}"),
                ProjectScope::AllProjects => "Search all projects".to_owned(),
            };
        }
        if self.scoped.filters != self.wider.filters {
            return "Clear filters".to_owned();
        }
        if self.scoped.sub_issues != self.wider.sub_issues {
            return "Show sub-issues".to_owned();
        }
        "Show all issues".to_owned()
    }
}

pub(super) fn changed(
    issues: &mut Issues,
    _: Entity<InputState>,
    event: &InputEvent,
    cx: &mut Context<Issues>,
) {
    if matches!(event, InputEvent::Change) {
        issues.search_changed(cx);
    }
}

// qual:allow(coupling, oi) reason: "Search input, temporary folds and page requests use the same retained Issues board state."

impl Issues {
    fn search_changed(&mut self, cx: &mut Context<Self>) {
        self.search_fallback = SearchFallback::Typing;
        self._search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_PAUSE).await;
            _ = this.update(cx, |this, cx| this.commit_search(cx));
        }));
        cx.notify();
    }

    pub(crate) fn commit_search(&mut self, cx: &mut Context<Self>) {
        let text = search_text(&self.search.read(cx).value());
        let previous = std::mem::take(&mut self.search_fallback);
        if text == self.query {
            if matches!(previous, SearchFallback::Typing) {
                self.check_hidden_matches(cx);
            } else {
                self.search_fallback = previous;
            }
            return;
        }
        self.query = text;
        self.search_folds.clear();
        self.reset_scope(cx);
    }

    pub(crate) fn cancel_search_fallback(&mut self) {
        self.search_fallback = SearchFallback::Idle;
    }

    pub(crate) fn invalidate_search_result(&mut self) {
        if !matches!(self.search_fallback, SearchFallback::Typing) {
            self.cancel_search_fallback();
        }
    }

    /// Check other allowed list choices only after the scoped result settles empty.
    pub(crate) fn check_hidden_matches(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.search_fallback, SearchFallback::Idle)
            || !self.search_is_empty()
            || self.query.is_empty()
            || search_text(&self.search.read(cx).value()) != self.query
        {
            return;
        }
        let Some(hub) = self.hub.clone() else { return };
        let scoped = self.choice();
        let candidates = candidates::search_candidates(&scoped, self.wider_projects(&scoped));
        if candidates.is_empty() {
            self.search_fallback = SearchFallback::Checked;
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            let query = scoped.query();
            let mut hidden = None;
            for wider in candidates {
                if let Some(found) = hub.find_wider_matches(&query, &wider.query()).await {
                    hidden = Some(HiddenMatch {
                        scoped: scoped.clone(),
                        wider,
                        found,
                    });
                    break;
                }
            }
            _ = this.update(cx, |this, cx| {
                this.finish_search_fallback(&scoped, hidden, cx)
            });
        });
        self.search_fallback = SearchFallback::Checking { _task: task };
        cx.notify();
    }

    fn search_is_empty(&self) -> bool {
        self.listing.status == Status::Idle
            && self.listing.issue_count() == Some(0)
            && !self.activity.visible
            && self.issue_view.snapshot.is_none()
    }

    fn finish_search_fallback(
        &mut self,
        scoped: &ListChoice,
        found: Option<HiddenMatch>,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.search_fallback, SearchFallback::Checking { .. })
            || &self.choice() != scoped
            || search_text(&self.search.read(cx).value()) != self.query
        {
            return;
        }
        if self.hub.as_ref().is_some_and(|hub| hub.account_changed()) {
            self.poll_hub(cx);
            return;
        }
        self.search_fallback = match found.filter(|_| self.search_is_empty()) {
            Some(hidden) => SearchFallback::Hidden(Box::new(hidden)),
            None => SearchFallback::Checked,
        };
        cx.notify();
    }

    pub(crate) fn search_recovery(&self) -> Option<&HiddenMatch> {
        match &self.search_fallback {
            SearchFallback::Hidden(hidden) if self.search_is_empty() => Some(hidden),
            _ => None,
        }
    }

    pub(crate) fn reveal_search(&mut self, cx: &mut Context<Self>) {
        if self.search_recovery().is_none() {
            return;
        }
        let SearchFallback::Hidden(hidden) = std::mem::take(&mut self.search_fallback) else {
            return;
        };
        if hidden.scoped != self.choice()
            || search_text(&self.search.read(cx).value()) != self.query
        {
            cx.notify();
            return;
        }
        let HiddenMatch { found, wider, .. } = *hidden;
        if !self
            .hub
            .as_ref()
            .is_some_and(|hub| hub.adopt_wider_matches(found))
        {
            self.poll_hub(cx);
            self.check_hidden_matches(cx);
            cx.notify();
            return;
        }
        self.tab = wider.tab;
        self.project = wider.project;
        self.sub_issues = wider.sub_issues;
        self.filters = wider.filters;
        prefs::set(cx, TAB_PREF, self.tab.key());
        prefs::set(cx, PROJECT_PREF, self.project.key());
        prefs::set(cx, SUB_ISSUES_PREF, self.sub_issues.key());
        self.rescope(cx);
    }

    fn wider_projects(&self, scoped: &ListChoice) -> Vec<ProjectScope> {
        use crate::initiative::project_in_scope;
        if scoped.project == ProjectScope::AllProjects {
            return Vec::new();
        }
        let all = ProjectScope::AllProjects;
        if project_in_scope(self, &all) {
            vec![all]
        } else {
            self.projects
                .iter()
                .map(|project| ProjectScope::Project(project.key.clone()))
                .filter(|project| *project != scoped.project && project_in_scope(self, project))
                .collect()
        }
    }

    /// Search folds are temporary. Results open even in normally folded states.
    pub(crate) fn group_folded(&self, group: &IssueGroup) -> bool {
        let saved = self.folds.is_folded(&group.key, group.kind);
        if self.query.is_empty() {
            return saved;
        }
        let has_matches = group.total.unwrap_or(0) > 0 || !group.rows.is_empty();
        self.search_folds
            .get(&group.key)
            .copied()
            .unwrap_or(saved && !has_matches)
    }

    pub(super) fn record_fold(
        &mut self,
        group: &str,
        kind: StateKind,
        folded: bool,
        cx: &mut Context<Self>,
    ) {
        if self.query.is_empty() {
            self.folds.record(group, kind, folded);
            prefs::set_list(cx, FOLDED_PREF, &self.folds.folded);
            prefs::set_list(cx, UNFOLDED_PREF, &self.folds.unfolded);
        } else {
            self.search_folds.insert(group.to_owned(), folded);
        }
    }

    /// Follow identifier-like searches through the existing scoped pages.
    /// Read one at a time and stop retrying a cursor if no rows were added.
    pub(crate) fn next_search_page(&mut self, groups: &[IssueGroup]) -> Option<String> {
        if !self.searches_for_identifier() {
            return None;
        }
        let group = groups.iter().find(|group| match group.more {
            More::Available => self.search_pages.get(&group.key) != Some(&group.rows.len()),
            More::Loading => true,
            _ => false,
        })?;
        if group.more == More::Loading {
            return None;
        }
        self.search_pages
            .insert(group.key.clone(), group.rows.len());
        Some(group.key.clone())
    }

    fn searches_for_identifier(&self) -> bool {
        let identifier = self.query.to_ascii_uppercase();
        ReferenceRecognizer::new(self.projects.iter().map(|project| project.key.as_str()), [])
            .recognises(&identifier)
    }
}
