//! A pull request's short summary for a reference's hover card. Optional
//! facts stay `None` when the responder has not read them.

/// Where a pull request stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PullRequestStatus {
    Open,
    Draft,
    Merged,
    Closed,
}

/// The review decision GitHub reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReviewStatus {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

/// The combined state of the head commit's checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckStatus {
    Success,
    Failure,
    Pending,
}

/// A short pull request summary, held only in memory for this account.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PullRequestDetails {
    /// The canonical `owner/repository#number`.
    pub key: String,
    pub title: String,
    /// The canonical pull request URL.
    pub url: String,
    pub status: PullRequestStatus,
    /// The author's login.
    pub author: Option<String>,
    pub review: Option<ReviewStatus>,
    pub checks: Option<CheckStatus>,
    /// When it last changed, as RFC 3339.
    pub updated_at: Option<String>,
    pub head_ref: Option<String>,
    pub base_ref: Option<String>,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

// qual:allow(dry, boilerplate) reason: "The record is #[non_exhaustive] so responders in other crates cannot build it literally; these setters are the contract's documented additive builder API (docs/board-integration.md), as IssueSummary's are."
impl PullRequestDetails {
    pub fn new(key: &str, title: &str, url: &str, status: PullRequestStatus) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            url: url.into(),
            status,
            author: None,
            review: None,
            checks: None,
            updated_at: None,
            head_ref: None,
            base_ref: None,
            additions: None,
            deletions: None,
        }
    }

    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_author(mut self, author: Option<&str>) -> Self {
        self.author = author.map(str::to_owned);
        self
    }

    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_review(mut self, review: Option<ReviewStatus>) -> Self {
        self.review = review;
        self
    }

    pub fn with_checks(mut self, checks: Option<CheckStatus>) -> Self {
        self.checks = checks;
        self
    }

    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_updated_at(mut self, updated_at: Option<&str>) -> Self {
        self.updated_at = updated_at.map(str::to_owned);
        self
    }

    /// The head branch and the branch it merges into.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_branches(mut self, head: Option<&str>, base: Option<&str>) -> Self {
        self.head_ref = head.map(str::to_owned);
        self.base_ref = base.map(str::to_owned);
        self
    }

    /// Lines added and deleted.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_changes(mut self, additions: Option<u32>, deletions: Option<u32>) -> Self {
        self.additions = additions;
        self.deletions = deletions;
        self
    }
}
