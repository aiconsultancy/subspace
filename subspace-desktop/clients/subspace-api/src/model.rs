//! Projections of Subspace API v1 OpenAPI. Serde field names are the Subspace API REST JSON contract.
//! Unknown fields remain forward compatible; no unused service enums are copied.

use serde::Deserialize;
mod activity;
mod comment;
mod debug;
mod decision;
mod initiative;
mod media;
mod person;
pub use decision::{
    AgentRank, DecisionDetail, DecisionEvidenceKind, DecisionIssue, DecisionIssues,
    DecisionListItem, DecisionMakerType, DecisionMember, DecisionProvenance, DecisionScope,
    DecisionStatus, DecisionSummary,
};
pub use initiative::{Fleet, FleetAgent, FleetShip, MemberInitiative};
pub use media::IssueMedia;
pub(crate) use media::MediaBinding;
pub use person::{Agent, Person};
mod pull_request;
pub use activity::{
    ActivityActor, ActivityActorType, ActivityKind, ActivityPage, ActivityReferences,
    IssueActivity, LastActivity,
};
pub use comment::{Comment, CommentProvenance};
pub use pull_request::{PullRequest, PullRequestRepository, PullRequestSummary};

/// Complete counts and bounded first pages, including empty terminal groups.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueGroups {
    pub group_by: String,
    pub groups: Vec<IssueGroup>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueGroup {
    pub state_id: Option<String>,
    pub state_type: String,
    pub count: u32,
    pub items: Vec<IssueListItem>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Project {
    pub id: String,
    pub key: String,
    pub name: String,
    pub default_state_id: String,
    pub initiative_id: Option<String>,
    pub archived_at: Option<String>,
    #[serde(skip)]
    pub states: Vec<WorkflowState>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct WorkflowState {
    pub id: String,
    pub project_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub position: i32,
    pub is_default: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueFields {
    pub id: String,
    pub identifier: String,
    pub project_id: String,
    pub project_key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub summary: Option<String>,
    pub state: StateSummary,
    pub assignee: Option<Person>,
    pub owner: Option<Person>,
    pub priority: i32,
    pub estimate: Option<f64>,
    pub due_date: Option<String>,
    pub labels: Vec<Label>,
    pub milestone_id: Option<String>,
    pub parent_id: Option<String>,
    pub blocked: bool,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version: i64,
    pub progress: Option<Progress>,
    pub comment_count: i32,
    pub todo_counts: TodoCounts,
    pub last_activity_at: String,
    pub last_activity: Option<LastActivity>,
    pub relayed_crew: Option<RelayedCrew>,
    pub provenance: Option<IssueProvenance>,
    pub cycle: Option<CycleSummary>,
    pub cycle_id: Option<String>,
    pub pull_request_summary: Option<PullRequestSummary>,
    /// NETT only: the planning project, apart from the issue team that the
    /// desktop calls the issue's project.
    #[serde(default)]
    pub planning_project: Option<PlanningProject>,
}

/// A NETT planning project an issue belongs to.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlanningProject {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueListItem {
    #[serde(flatten)]
    pub fields: IssueFields,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueDetail {
    #[serde(flatten)]
    pub fields: IssueFields,
    pub description: Option<String>,
    #[serde(default)]
    pub mentions: Vec<MentionStatus>,
    pub todos: Vec<Todo>,
    pub created_by: Option<Person>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub canceled_at: Option<String>,
    pub archived_by: Option<Person>,
    pub parent: Option<IssueReference>,
    pub duplicate_of: Option<IssueReference>,
    pub children: Option<Page<IssueListItem>>,
    pub relations: Option<Page<Relation>>,
}

/// Active members may have canRead or cannotRead status. notMember has no name.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MentionStatus {
    pub user_id: String,
    pub status: String,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueProvenance {
    pub source_system: String,
    pub original_identifier: String,
    pub original_url: Option<String>,
    pub original_creator_name: Option<String>,
    pub original_created_at: Option<String>,
    pub original_started_at: Option<String>,
    pub original_completed_at: Option<String>,
    pub original_canceled_at: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct StateSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: String,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Progress {
    pub done: i32,
    pub total: i32,
    pub canceled: i32,
    pub duplicate: i32,
    pub sample: Vec<ProgressSample>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ProgressSample {
    pub id: String,
    pub state_type: String,
}

#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct CycleSummary {
    pub id: String,
    pub number: i32,
    pub state: String,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct IssueReference {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub summary: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub state: StateSummary,
    pub archived: bool,
}

#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub struct Relation {
    #[serde(flatten)]
    pub issue: IssueListItem,
    pub kind: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Me {
    pub user_id: String,
    pub kind: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct TodoCounts {
    pub total: i32,
    pub done: i32,
}

#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub struct RelayedCrew {
    pub name: String,
    pub model: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Todo {
    pub id: String,
    pub issue_id: String,
    pub text: String,
    pub done: bool,
    pub position: i32,
    pub created_by: Person,
    pub created_at: String,
    pub done_by: Option<Person>,
    pub done_at: Option<String>,
    pub updated_by: Option<Person>,
    pub updated_at: String,
    pub version: i64,
    pub relayed_crew: Option<RelayedCrew>,
    pub updated_relayed_crew: Option<RelayedCrew>,
}

impl std::ops::Deref for IssueListItem {
    type Target = IssueFields;
    fn deref(&self) -> &IssueFields {
        &self.fields
    }
}
