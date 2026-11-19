//! The GitHub data model shown by the GitHub board. Field names follow the
//! JSON (camelCase).

use serde::{Deserialize, Serialize};

mod helpers;

pub use helpers::{Image, SECTIONS, SectionDef, display_name, image_type, short_login};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Pr,
    Issue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhUser {
    pub login: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhLabel {
    pub name: String,
    /// Hex without `#`.
    pub color: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

/// Issue states: a subset of [`PrState`], which [`GhItem::state`] uses.
// qual:api (public serde contract)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    Success,
    Failure,
    Pending,
}

/// One row. `id` is the GraphQL node id (used for writes); `key` is
/// `owner/name#number`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhItem {
    pub id: String,
    pub key: String,
    pub kind: ItemKind,
    /// `owner/name`.
    pub repo: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    /// `PrState` or `IssueState`; issues use only `open` and `closed`.
    pub state: PrState,
    pub author: Option<GhUser>,
    pub assignees: Vec<GhUser>,
    pub labels: Vec<GhLabel>,
    pub comments: u32,
    pub created_at: String,
    pub updated_at: String,
    // PR only (`None` for issues).
    pub review_decision: Option<ReviewDecision>,
    /// `None`: no checks.
    pub checks: Option<CheckState>,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    pub head_ref: Option<String>,
    pub base_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckRunState {
    Success,
    Failure,
    Pending,
    Neutral,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhCheck {
    pub name: String,
    pub state: CheckRunState,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerState {
    Approved,
    ChangesRequested,
    Commented,
    Pending,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhReviewer {
    pub user: GhUser,
    pub state: ReviewerState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Comment,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhEvent {
    pub kind: EventKind,
    pub author: Option<GhUser>,
    pub created_at: String,
    /// GitHub `bodyHTML`; sanitize before display.
    pub body_html: String,
    /// For reviews.
    pub review_state: Option<ReviewerState>,
    pub url: Option<String>,
}

/// `GET /api/gh/item?account=<login>&repo=<owner/name>&number=<n>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhItemDetail {
    #[serde(flatten)]
    pub item: GhItem,
    pub body_html: String,
    /// Latest state per reviewer, plus requested reviewers as `pending` (PRs).
    pub reviewers: Vec<GhReviewer>,
    /// PRs, latest commit.
    pub check_runs: Vec<GhCheck>,
    /// Comments and reviews, newest first (up to 100).
    pub events: Vec<GhEvent>,
    pub viewer_can_review: bool,
    pub viewer_can_comment: bool,
    /// PRs.
    pub changed_files: Option<u32>,
}

/// `GET /api/gh/section`: up to 50 items, most recently updated first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionResponse {
    pub account: String,
    pub section: String,
    /// GitHub `issueCount`; may exceed `items.len()`.
    pub total: u32,
    pub items: Vec<GhItem>,
}

/// A repository from GitHub's repository search or `GET /user/repos`. Field
/// names follow GitHub's REST JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhRepository {
    /// `owner/name`.
    pub full_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub archived: bool,
}

/// `GET /api/gh/assignable`: people who can be assigned (up to 100).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignableResponse {
    pub users: Vec<GhUser>,
}

/// The review of `POST /api/gh/review`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

impl ReviewEvent {
    /// The GraphQL `PullRequestReviewEvent` value.
    // qual:allow(dry, repeated_matches) reason: "Exhaustive enum mapping is intentional; the test covers every variant."
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::RequestChanges => "REQUEST_CHANGES",
            Self::Comment => "COMMENT",
        }
    }
}

/// GitHub's file status, as the pulls files API reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GhFileStatus {
    Added,
    Removed,
    Modified,
    Renamed,
    Copied,
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhDiffFile {
    /// The path in the head commit (for `removed`, the deleted path).
    pub path: String,
    /// Renamed and copied files.
    pub previous_path: Option<String>,
    pub status: GhFileStatus,
    pub additions: u32,
    pub deletions: u32,
    /// Unified hunks without file headers; `None` when GitHub omits them
    /// (binary or large files).
    pub patch: Option<String>,
}

/// `GET /api/gh/diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResponse {
    pub repo: String,
    pub number: u32,
    /// The merge base: the old side of every patch.
    pub base_sha: String,
    pub head_sha: String,
    /// Changed files on GitHub.
    pub total: u32,
    /// Up to 3,000 (GitHub's limit).
    pub files: Vec<GhDiffFile>,
}

/// `GET /api/gh/file`: one file's text, up to 1 MB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResponse {
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::{
        CheckState, GhItem, GhUser, ItemKind, PrState, ReviewDecision, display_name, image_type,
        short_login,
    };

    #[test]
    fn emu_logins_lose_the_shortcode() {
        assert_eq!(short_login("Alex-J-Example_emu"), "Alex-J-Example");
        assert_eq!(short_login("ann"), "ann");
        assert_eq!(short_login("a_b_C1"), "a_b");
        assert_eq!(short_login("trailing_"), "trailing_");
        let user = GhUser {
            login: "ann_x".into(),
            name: Some("  ".into()),
        };
        assert_eq!(display_name(&user), "ann");
    }

    #[test]
    fn image_types_by_extension() {
        assert_eq!(image_type("a/b.PNG"), Some("image/png"));
        assert_eq!(image_type("x.svg"), Some("image/svg+xml"));
        assert_eq!(image_type("x.rs"), None);
        assert_eq!(image_type("dir.png/file"), None);
        assert_eq!(image_type("noext"), None);
    }

    #[test]
    fn a_pull_request_parses_from_the_work_api_json() {
        let json = r#"{
            "id": "PR_kwDO", "key": "octo/app#12", "kind": "pr", "repo": "octo/app",
            "number": 12, "title": "Add the list", "url": "https://github.com/octo/app/pull/12",
            "state": "draft", "author": {"login": "ann", "name": null},
            "assignees": [], "labels": [{"name": "ui", "color": "a2eeef"}], "comments": 2,
            "createdAt": "2026-09-01T10:00:00Z", "updatedAt": "2026-09-02T10:00:00Z",
            "reviewDecision": "changes_requested", "checks": null,
            "additions": 10, "deletions": 3, "headRef": "list", "baseRef": "main"
        }"#;
        let item: GhItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.kind, ItemKind::Pr);
        assert_eq!(item.state, PrState::Draft);
        assert_eq!(item.review_decision, Some(ReviewDecision::ChangesRequested));
        assert_eq!(item.checks, None::<CheckState>);
        assert_eq!(item.labels[0].color, "a2eeef");
    }

    #[test]
    fn review_events_use_the_graphql_names() {
        assert_eq!(super::ReviewEvent::Approve.as_str(), "APPROVE");
        assert_eq!(
            super::ReviewEvent::RequestChanges.as_str(),
            "REQUEST_CHANGES"
        );
        assert_eq!(super::ReviewEvent::Comment.as_str(), "COMMENT");
    }
}
