//! Linked pull requests. Issue associations are served separately from issue detail.
use serde::Deserialize;

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PullRequestSummary {
    pub total: i32,
    pub open: i32,
    pub closed: i32,
    pub merged: i32,
    pub closing_pending: i32,
    pub checks_state: Option<String>,
    pub last_observed_at: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PullRequestRepository {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub url: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PullRequest {
    pub id: String,
    pub repository: PullRequestRepository,
    pub number: i64,
    pub url: String,
    pub title: String,
    pub author_login: Option<String>,
    pub state: String,
    pub is_draft: bool,
    pub head_branch: String,
    pub base_branch: String,
    pub reference_kind: String,
    pub reference_sources: Vec<String>,
    pub reference_active: bool,
    pub completion: String,
    pub merged_at: Option<String>,
    pub closed_at: Option<String>,
    pub github_updated_at: String,
    pub last_observed_at: String,
}
