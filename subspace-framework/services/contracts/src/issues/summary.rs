//! Optional reference-card facts. Older responders can keep supplying only the title and state.

use super::IssueState;

mod details;
mod facts;
pub use facts::{IssuePerson, IssueProgress, LatestActivity, PullRequestSummary};

/// A short issue summary, held only within the current viewer session.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IssueSummary {
    pub identifier: String,
    pub title: String,
    /// Plain text explaining the problem this issue solves; absent on older issues.
    pub summary: Option<String>,
    /// The issue's navigation URL, handled by the responding board.
    pub url: String,
    pub state: IssueState,
    pub tracker: bool,
    pub archived: bool,
    /// The assignee's display name.
    pub assignee: Option<String>,
    details: Option<Box<details::Details>>,
}

impl IssueSummary {
    pub fn new(identifier: &str, title: &str, url: &str, state: IssueState) -> Self {
        Self {
            identifier: identifier.into(),
            title: title.into(),
            summary: None,
            url: url.into(),
            state,
            tracker: false,
            archived: false,
            assignee: None,
            details: None,
        }
    }

    /// Whether the issue is a tracker and whether it is archived.
    pub fn with_markers(mut self, tracker: bool, archived: bool) -> Self {
        self.tracker = tracker;
        self.archived = archived;
        self
    }

    pub fn with_summary(mut self, summary: Option<&str>) -> Self {
        self.summary = summary.map(str::to_owned);
        self
    }

    pub fn with_assignee(mut self, assignee: Option<&str>) -> Self {
        self.assignee = assignee.map(str::to_owned);
        self
    }
}
