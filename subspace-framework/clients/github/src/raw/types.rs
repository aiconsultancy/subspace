use serde::Deserialize;

use crate::model::GhLabel;

/// A GraphQL connection's `nodes`, where any node may be `null`.
#[derive(Debug, Default, Deserialize)]
pub struct Nodes<T> {
    #[serde(default = "Vec::new")]
    pub nodes: Vec<Option<T>>,
}

impl<T> Nodes<T> {
    pub(super) fn present(&self) -> impl Iterator<Item = &T> {
        self.nodes.iter().flatten()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawActor {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRepository {
    pub name_with_owner: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCount {
    pub total_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRollup {
    pub state: Option<String>,
    #[serde(default)]
    pub contexts: Option<Nodes<RawContext>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCommit {
    pub status_check_rollup: Option<RawRollup>,
}

#[derive(Debug, Deserialize)]
pub struct RawCommitNode {
    pub commit: RawCommit,
}

#[derive(Debug, Default, Deserialize)]
#[serde(tag = "__typename")]
pub enum RawContext {
    #[serde(rename_all = "camelCase")]
    CheckRun {
        name: String,
        status: String,
        conclusion: Option<String>,
        details_url: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    StatusContext {
        context: String,
        state: String,
        target_url: Option<String>,
    },
    #[serde(other)]
    #[default]
    Other,
}

/// The fields every row has, plus the PR row fields.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCommon {
    #[serde(rename = "__typename")]
    pub typename: String,
    pub id: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub repository: RawRepository,
    pub author: Option<RawActor>,
    #[serde(default)]
    pub assignees: Nodes<RawActor>,
    #[serde(default)]
    pub labels: Option<Nodes<GhLabel>>,
    #[serde(default)]
    pub comments: RawCount,
    #[serde(default)]
    pub is_draft: Option<bool>,
    #[serde(default)]
    pub additions: Option<u32>,
    #[serde(default)]
    pub deletions: Option<u32>,
    #[serde(default)]
    pub head_ref_name: Option<String>,
    #[serde(default)]
    pub base_ref_name: Option<String>,
    #[serde(default)]
    pub review_decision: Option<String>,
    #[serde(default)]
    pub commits: Option<Nodes<RawCommitNode>>,
}

/// The fields of the extra PR searches, merged into rows by node id.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawExtra {
    pub id: String,
    #[serde(default)]
    pub review_decision: Option<String>,
    #[serde(default)]
    pub additions: Option<u32>,
    #[serde(default)]
    pub deletions: Option<u32>,
}

impl RawCommon {
    pub(super) fn merge(&mut self, extra: &RawExtra) {
        self.review_decision = extra
            .review_decision
            .clone()
            .or(self.review_decision.take());
        self.additions = extra.additions.or(self.additions);
        self.deletions = extra.deletions.or(self.deletions);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawReview {
    pub author: Option<RawActor>,
    pub state: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub submitted_at: Option<String>,
    #[serde(default, rename = "bodyHTML")]
    pub body_html: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawComment {
    pub author: Option<RawActor>,
    pub created_at: String,
    #[serde(rename = "bodyHTML")]
    pub body_html: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum RawRequested {
    User(RawActor),
    Bot(RawActor),
    Mannequin(RawActor),
    #[serde(rename_all = "camelCase")]
    Team {
        combined_slug: String,
        name: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRequest {
    pub requested_reviewer: Option<RawRequested>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDetail {
    #[serde(flatten)]
    pub common: RawCommon,
    #[serde(rename = "bodyHTML")]
    pub body_html: String,
    #[serde(default)]
    pub viewer_did_author: bool,
    #[serde(default)]
    pub viewer_can_react: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub changed_files: Option<u32>,
    #[serde(default)]
    pub comment_events: Option<Nodes<RawComment>>,
    #[serde(default)]
    pub latest_reviews: Option<Nodes<RawReview>>,
    #[serde(default)]
    pub latest_opinionated_reviews: Option<Nodes<RawReview>>,
    #[serde(default)]
    pub review_requests: Option<Nodes<RawRequest>>,
    #[serde(default)]
    pub reviews: Option<Nodes<RawReview>>,
}

/// A file of the pulls files API.
#[derive(Debug, Deserialize)]
pub struct RawPrFile {
    pub filename: String,
    #[serde(default)]
    pub previous_filename: Option<String>,
    pub status: String,
    #[serde(default)]
    pub additions: Option<u32>,
    #[serde(default)]
    pub deletions: Option<u32>,
    #[serde(default)]
    pub patch: Option<String>,
}

pub(super) use RawComment as Comment;
pub(super) use RawReview as Review;
