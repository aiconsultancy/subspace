//! Optional details are boxed together so the existing resolution enum stays small.
use super::{IssuePerson, IssueProgress, IssueSummary, LatestActivity, PullRequestSummary};
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Details {
    pub priority: Option<u8>,
    pub owner: Option<IssuePerson>,
    pub assignee_id: Option<String>,
    pub assignee_kind: Option<String>,
    pub assignee_rank: Option<String>,
    pub progress: Option<IssueProgress>,
    pub pull_requests: Option<PullRequestSummary>,
    pub due_date: Option<String>,
    pub latest_activity: Option<LatestActivity>,
}

impl IssueSummary {
    pub fn with_priority(mut self, value: Option<u8>) -> Self {
        self.details.get_or_insert_with(Default::default).priority = value;
        self
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn priority(&self) -> Option<u8> {
        self.details.as_ref().and_then(|facts| facts.priority)
    }
    pub fn with_owner(mut self, value: Option<IssuePerson>) -> Self {
        self.details.get_or_insert_with(Default::default).owner = value;
        self
    }
    pub fn owner(&self) -> Option<&IssuePerson> {
        self.details.as_ref().and_then(|facts| facts.owner.as_ref())
    }
    /// Stable identity allows an owner comparison even when two people share a name.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_assignee_id(mut self, value: Option<&str>) -> Self {
        self.details
            .get_or_insert_with(Default::default)
            .assignee_id = value.map(str::to_owned);
        self
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn assignee_id(&self) -> Option<&str> {
        self.details
            .as_ref()
            .and_then(|facts| facts.assignee_id.as_deref())
    }
    pub fn with_assignee_kind(mut self, value: Option<&str>) -> Self {
        self.details
            .get_or_insert_with(Default::default)
            .assignee_kind = value.map(str::to_owned);
        self
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn assignee_kind(&self) -> Option<&str> {
        self.details
            .as_ref()
            .and_then(|facts| facts.assignee_kind.as_deref())
    }
    pub fn with_assignee_rank(mut self, value: Option<&str>) -> Self {
        self.details
            .get_or_insert_with(Default::default)
            .assignee_rank = value.map(str::to_owned);
        self
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn assignee_rank(&self) -> Option<&str> {
        self.details
            .as_ref()
            .and_then(|facts| facts.assignee_rank.as_deref())
    }
    pub fn with_progress(mut self, value: Option<IssueProgress>) -> Self {
        self.details.get_or_insert_with(Default::default).progress = value;
        self
    }
    pub fn progress(&self) -> Option<IssueProgress> {
        self.details.as_ref().and_then(|facts| facts.progress)
    }
    pub fn with_pull_requests(mut self, value: Option<PullRequestSummary>) -> Self {
        self.details
            .get_or_insert_with(Default::default)
            .pull_requests = value;
        self
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn pull_requests(&self) -> Option<&PullRequestSummary> {
        self.details
            .as_ref()
            .and_then(|facts| facts.pull_requests.as_ref())
    }
    pub fn with_due_date(mut self, value: Option<&str>) -> Self {
        self.details.get_or_insert_with(Default::default).due_date = value.map(str::to_owned);
        self
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn due_date(&self) -> Option<&str> {
        self.details
            .as_ref()
            .and_then(|facts| facts.due_date.as_deref())
    }
    pub fn with_latest_activity(mut self, value: Option<LatestActivity>) -> Self {
        self.details
            .get_or_insert_with(Default::default)
            .latest_activity = value;
        self
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn latest_activity(&self) -> Option<&LatestActivity> {
        self.details
            .as_ref()
            .and_then(|facts| facts.latest_activity.as_ref())
    }
}
