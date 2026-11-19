use super::ReadError;
use crate::model::IssueDetail;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueStatus {
    Loading,
    Loaded,
    /// Neutral unavailable wording, including a 404 after the pack check.
    Unavailable,
    Error(ReadError),
}

/// Memory-only open issue, independent of the list and IRC projection.
#[derive(Clone)]
pub struct IssueSnapshot {
    pub(in crate::hub) identifier: String,
    pub(in crate::hub) profile_id: String,
    pub(in crate::hub) status: IssueStatus,
    pub(in crate::hub) detail: Option<IssueDetail>,
    pub(in crate::hub) milestone_name: Option<String>,
    pub(in crate::hub) milestone_loading: bool,
    pub(in crate::hub) updated_at: Option<Instant>,
    pub(in crate::hub) children_loading: bool,
    pub(in crate::hub) children_failed: bool,
    pub(in crate::hub) relations_loading: bool,
    pub(in crate::hub) relations_failed: bool,
    /// Bumped whenever the sub-issue or relation page is replaced; a
    /// continuation applies only to the epoch it was asked for.
    pub(in crate::hub) epochs: Epochs,
    pub(in crate::hub) pull_requests: crate::hub::pull_requests::PullRequests,
    pub(in crate::hub) decisions: crate::hub::decisions::IssueDecisions,
    pub(in crate::hub) decision: crate::hub::decision::Slot,
    pub(in crate::hub) timeline: TimelineSnapshot,
    pub(in crate::hub) images: crate::hub::ImagesSnapshot,
    pub(in crate::hub) files: crate::hub::FilesSnapshot,
}

impl IssueSnapshot {
    pub fn pull_requests(&self) -> Option<&crate::model::Page<crate::model::PullRequest>> {
        self.pull_requests.page.as_ref()
    }
    pub fn pull_requests_loading(&self) -> bool {
        self.pull_requests.loading
    }
    pub fn pull_requests_failed(&self) -> bool {
        self.pull_requests.error.is_some()
    }
    pub fn pull_requests_unavailable(&self) -> bool {
        self.pull_requests.unavailable
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn images(&self) -> &crate::hub::ImagesSnapshot {
        &self.images
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn files(&self) -> &crate::hub::FilesSnapshot {
        &self.files
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn timeline(&self) -> &TimelineSnapshot {
        &self.timeline
    }

    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn relations_loading(&self) -> bool {
        self.relations_loading
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn relations_failed(&self) -> bool {
        self.relations_failed
    }

    pub(in crate::hub) fn new(identifier: &str, profile_id: &str, status: IssueStatus) -> Self {
        Self {
            identifier: identifier.into(),
            profile_id: profile_id.into(),
            status,
            detail: None,
            milestone_name: None,
            milestone_loading: false,
            updated_at: None,
            children_loading: false,
            children_failed: false,
            relations_loading: false,
            relations_failed: false,
            epochs: Epochs::default(),
            pull_requests: Default::default(),
            decisions: Default::default(),
            decision: Default::default(),
            timeline: TimelineSnapshot::default(),
            images: Default::default(),
            files: Default::default(),
        }
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn status(&self) -> &IssueStatus {
        &self.status
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn milestone_loading(&self) -> bool {
        self.milestone_loading
    }
    pub fn milestone_name(&self) -> Option<&str> {
        self.milestone_name.as_deref()
    }
    pub fn detail(&self) -> Option<&IssueDetail> {
        self.detail.as_ref()
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn updated_at(&self) -> Option<Instant> {
        self.updated_at
    }
    // qual:api — consumed by the issue board in slice 3b.
    pub fn age(&self, now: Instant) -> Option<Duration> {
        self.updated_at.map(|at| now.saturating_duration_since(at))
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn children_loading(&self) -> bool {
        self.children_loading
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn children_failed(&self) -> bool {
        self.children_failed
    }
}

/// Installation counters for the detail's paged sub-issues and relations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::hub) struct Epochs {
    pub children: u64,
    pub relations: u64,
}

impl Epochs {
    /// Both pages were replaced.
    pub(in crate::hub) fn replaced(&mut self) {
        self.children = self.children.wrapping_add(1);
        self.relations = self.relations.wrapping_add(1);
    }
}

#[derive(Clone, Default)]
pub struct TimelineSnapshot {
    pub(in crate::hub) epoch: u64,
    pub(in crate::hub) content: TimelineContent,
    pub(in crate::hub) loading: bool,
    pub(in crate::hub) error: Option<ReadError>,
}

/// Loaded activity, its reference index and optimistic comments share one lifetime.
#[derive(Clone, Default)]
pub(in crate::hub) struct TimelineContent {
    pub(in crate::hub) request_sequence: u64,
    pub(in crate::hub) decision_sequences: std::collections::HashMap<String, u64>,
    pub(in crate::hub) decisions: std::collections::HashMap<String, crate::model::DecisionSummary>,
    pub(in crate::hub) projected: Vec<crate::model::Comment>,
    pub(in crate::hub) page: Option<crate::model::Page<crate::model::IssueActivity>>,
}
impl TimelineSnapshot {
    /// A detail reload replaces the timeline without navigating away.
    pub(in crate::hub) fn reset(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.content = TimelineContent::default();
        self.loading = false;
        self.error = None;
    }

    pub fn decision(&self, id: &str) -> Option<&crate::model::DecisionSummary> {
        self.content.decisions.get(id)
    }
    pub fn projected(&self) -> &[crate::model::Comment] {
        &self.content.projected
    }
    pub fn page(&self) -> Option<&crate::model::Page<crate::model::IssueActivity>> {
        self.content.page.as_ref()
    }
    pub fn loading(&self) -> bool {
        self.loading
    }
    pub fn error(&self) -> Option<&ReadError> {
        self.error.as_ref()
    }
    pub fn idle(&self) -> bool {
        self.content.page.is_none() && !self.loading && self.error.is_none()
    }
}

impl std::fmt::Debug for IssueSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueSnapshot")
            .field("identifier", &self.identifier)
            .field("profile_id", &self.profile_id)
            .field("status", &self.status)
            .field("detail", &self.detail)
            .field("timeline", &self.timeline)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for TimelineSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimelineSnapshot")
            .field("decision_count", &self.content.decisions.len())
            .field("projected_count", &self.content.projected.len())
            .field("page", &self.content.page)
            .field("loading", &self.loading)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
