use super::{ReadError, STALE_AFTER, ScopeGeneration, State};
use crate::{
    ActivityQuery,
    model::{IssueListItem, Page},
};
use std::time::Instant;
pub(in crate::hub) mod merge;

/// Each section has its own cursor, loading, error and Retry state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivitySection {
    Trackers,
    Recent,
}

#[derive(Clone, Default)]
// qual:allow(srp, god_struct) reason: "First-page and continuation status form one independently retryable section."
pub struct ActivityListSnapshot {
    pub(in crate::hub) page: Option<Page<IssueListItem>>,
    pub(in crate::hub) loading: bool,
    pub(in crate::hub) error: Option<ReadError>,
    pub(in crate::hub) page_loading: bool,
    pub(in crate::hub) page_error: Option<ReadError>,
    pub(in crate::hub) updated_at: Option<Instant>,
    pub(in crate::hub) head_pending: bool,
}

impl ActivityListSnapshot {
    pub fn page(&self) -> Option<&Page<IssueListItem>> {
        self.page.as_ref()
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn loading(&self) -> bool {
        self.loading
    }
    pub fn error(&self) -> Option<&ReadError> {
        self.error.as_ref()
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn page_loading(&self) -> bool {
        self.page_loading
    }
    pub fn page_error(&self) -> Option<&ReadError> {
        self.page_error.as_ref()
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn updated_at(&self) -> Option<Instant> {
        self.updated_at
    }
    pub(in crate::hub) fn begin(&mut self, later: bool) {
        if later {
            self.page_loading = true;
            self.page_error = None;
        } else {
            self.head_pending = false;
            self.loading = true;
            self.error = None;
            self.page_loading = false;
            self.page_error = None;
        }
    }
    pub(in crate::hub) fn fail(&mut self, later: bool, reason: ReadError) {
        if later {
            self.page_loading = false;
            self.page_error = Some(reason);
        } else {
            self.loading = false;
            self.error = Some(reason);
            self.updated_at = Some(Instant::now());
        }
    }
}

#[derive(Clone)]
pub struct ActivitySnapshot {
    query: ActivityQuery,
    trackers: ActivityListSnapshot,
    recent: ActivityListSnapshot,
}

impl ActivitySnapshot {
    pub(in crate::hub) fn new(query: ActivityQuery) -> Self {
        Self {
            query,
            trackers: ActivityListSnapshot::default(),
            recent: ActivityListSnapshot::default(),
        }
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn query(&self) -> &ActivityQuery {
        &self.query
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn trackers(&self) -> &ActivityListSnapshot {
        &self.trackers
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn recent(&self) -> &ActivityListSnapshot {
        &self.recent
    }
    pub fn section(&self, section: ActivitySection) -> &ActivityListSnapshot {
        match section {
            ActivitySection::Trackers => &self.trackers,
            ActivitySection::Recent => &self.recent,
        }
    }
    pub(in crate::hub) fn section_mut(
        &mut self,
        section: ActivitySection,
    ) -> &mut ActivityListSnapshot {
        match section {
            ActivitySection::Trackers => &mut self.trackers,
            ActivitySection::Recent => &mut self.recent,
        }
    }
    pub(in crate::hub) fn stale(&self, now: Instant) -> bool {
        [&self.trackers, &self.recent].iter().any(|list| {
            !list.loading
                && !list.head_pending
                && list
                    .updated_at
                    .is_none_or(|at| now.saturating_duration_since(at) > STALE_AFTER)
        })
    }
}

impl State {
    pub(in crate::hub) fn activity_current(
        &self,
        generation: ScopeGeneration,
        revision: u64,
        section: ActivitySection,
    ) -> bool {
        self.current(generation) && self.activity_revision[section as usize] == revision
    }
    pub(in crate::hub) fn clear_activity(&mut self) {
        self.activity_revision = self
            .activity_revision
            .map(|revision| revision.wrapping_add(1));
        self.pending_activity = None;
        self.snapshot.activity = None;
    }
    pub(in crate::hub) fn select_project(&mut self, project: Option<&str>) {
        if self.snapshot.selected_project.as_deref() == project {
            return;
        }
        self.snapshot.selected_project = project.map(str::to_owned);
        self.clear_activity();
        self.clear_list();
        self.query = self.query.take().map(|query| query.project(project));
        self.changed();
    }

    pub(in crate::hub) fn has_project_scope(&self) -> bool {
        self.snapshot.selected_project.is_some()
            || self.query.is_some()
            || self.pending_activity.is_some()
            || self.snapshot.activity.is_some()
    }

    pub(in crate::hub) fn clear_list(&mut self) {
        self.snapshot.list = None;
        self.snapshot.list_loading = false;
        self.snapshot.updated_at = None;
        self.snapshot.hidden_subissues_count = None;
        self.snapshot.my_issues_count = None;
        self.snapshot.page_loading.clear();
        self.snapshot.page_errors.clear();
        self.revision = self.revision.wrapping_add(1);
    }
}

impl std::fmt::Debug for ActivityListSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityListSnapshot")
            .field("page", &self.page)
            .field("loading", &self.loading)
            .field("error", &self.error)
            .field("page_loading", &self.page_loading)
            .field("page_error", &self.page_error)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ActivitySnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivitySnapshot")
            .field("query", &self.query)
            .field("trackers", &self.trackers)
            .field("recent", &self.recent)
            .finish_non_exhaustive()
    }
}
