//! The issue-list search is scoped to the active board and debounced.
use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{Context, Entity, Task};

use subspace_api_client::hub::WiderSearch;
use subspace_framework::contracts::issues::recognition::ReferenceRecognizer;
use subspace_framework::dashboard::prefs;

use super::{FOLDED_PREF, TAB_PREF, UNFOLDED_PREF};
use crate::Issues;
use crate::listing::{IssueGroup, More, Status};
use crate::model::{StateKind, Tab};
use crate::requests::{ListChoice, search_text};

/// Typing waits this long before the list is read again.
const SEARCH_PAUSE: Duration = Duration::from_millis(300);

#[derive(Default)]
pub(crate) enum SearchFallback {
    #[default]
    Idle,
    Typing,
    AwaitingList,
    Checking {
        _task: Task<()>,
    },
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
    }

    pub(crate) fn commit_search(&mut self, cx: &mut Context<Self>) {
        let text = search_text(&self.search.read(cx).value());
        if text == self.query {
            return;
        }
        let widen = matches!(self.search_fallback, SearchFallback::Typing)
            && self.tab != Tab::All
            && !text.is_empty();
        self.search_fallback = if widen {
            SearchFallback::AwaitingList
        } else {
            SearchFallback::Idle
        };
        self.query = text;
        self.search_folds.clear();
        self.reset_scope(cx);
    }

    pub(crate) fn cancel_search_fallback(&mut self) {
        self.search_fallback = SearchFallback::Idle;
    }

    /// Widen a newly typed search only after its scoped read succeeds with no matches.
    pub(crate) fn maybe_search_all(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.search_fallback, SearchFallback::AwaitingList)
            || self.listing.status == Status::Loading
            || self.listing.content.is_none()
        {
            return;
        }
        self.cancel_search_fallback();
        if !self.search_is_empty() {
            return;
        }
        let Some(hub) = self.hub.clone() else {
            return;
        };
        let scoped = self.choice();
        let task = cx.spawn(async move |this, cx| {
            let found = hub.find_wider_matches(&scoped.query()).await;
            _ = this.update(cx, |this, cx| {
                this.finish_search_fallback(&scoped, found, cx)
            });
        });
        self.search_fallback = SearchFallback::Checking { _task: task };
    }

    fn search_is_empty(&self) -> bool {
        self.listing.status == Status::Idle && self.listing.issue_count() == Some(0)
    }

    fn finish_search_fallback(
        &mut self,
        scoped: &ListChoice,
        found: Option<WiderSearch>,
        cx: &mut Context<Self>,
    ) {
        self.cancel_search_fallback();
        let (Some(found), Some(hub)) = (found, self.hub.as_ref()) else {
            return;
        };
        if !self.search_is_empty()
            || &self.choice() != scoped
            || self.activity.visible
            || self.issue_view.snapshot.is_some()
            || search_text(&self.search.read(cx).value()) != self.query
            || !hub.adopt_wider_matches(found)
        {
            return;
        }
        // Keep the toolbar's explicit filters, including those set in the original tab.
        self.tab = Tab::All;
        prefs::set(cx, TAB_PREF, self.tab.key());
        self.rescope(cx);
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
