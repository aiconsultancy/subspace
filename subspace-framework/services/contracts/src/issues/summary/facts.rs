//! Additive data records without service dependencies or logging implementations.

#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IssuePerson {
    pub id: String,
    pub name: String,
    pub kind: String,
}
impl IssuePerson {
    pub fn new(id: &str, name: &str, kind: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct IssueProgress {
    pub done: u32,
    pub total: u32,
}
impl IssueProgress {
    pub fn new(done: u32, total: u32) -> Self {
        Self {
            done: done.min(total),
            total,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PullRequestSummary {
    pub total: u32,
    pub open: u32,
    pub closed: u32,
    pub merged: u32,
    /// The service's checks state; unknown values remain forward compatible.
    pub checks: Option<String>,
}
impl PullRequestSummary {
    pub fn new(total: u32, open: u32, closed: u32, merged: u32) -> Self {
        Self {
            total,
            open,
            closed,
            merged,
            checks: None,
        }
    }
    pub fn with_checks(mut self, checks: Option<&str>) -> Self {
        self.checks = checks.map(str::to_owned);
        self
    }
}

#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LatestActivity {
    pub kind: String,
    /// RFC 3339 service time. Parsing and relative formatting belong to the viewer.
    pub occurred_at: String,
    pub actor: Option<String>,
}
impl LatestActivity {
    pub fn new(kind: &str, occurred_at: &str) -> Self {
        Self {
            kind: kind.into(),
            occurred_at: occurred_at.into(),
            actor: None,
        }
    }
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn with_actor(mut self, actor: Option<&str>) -> Self {
        self.actor = actor.map(str::to_owned);
        self
    }
}
