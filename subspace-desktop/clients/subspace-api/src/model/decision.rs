//! Read-only decision projections from the API's Decisions/Shared contract.
use chrono::NaiveDate;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum DecisionStatus {
    Proposed,
    Accepted,
    Rejected,
    Superseded,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum DecisionMakerType {
    Human,
    Agent,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum AgentRank {
    RearAdmiral,
    Captain,
    Crew,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum DecisionEvidenceKind {
    IrcMessage,
    TeamsMessage,
    SubspaceComment,
    DocumentOrUrl,
    Meeting,
    #[serde(other)]
    Other,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionListItem {
    pub id: String,
    pub title: String,
    pub status: DecisionStatus,
    pub decided_on: String,
    pub project_id: String,
    pub project_key: String,
    #[serde(default)]
    pub project: Option<DecisionScope>,
    #[serde(default)]
    pub supersedes_id: Option<String>,
    #[serde(default)]
    pub superseded_by_id: Option<String>,
    #[serde(default)]
    pub supersedes: Option<DecisionSummary>,
    #[serde(default)]
    pub superseded_by: Option<DecisionSummary>,
    pub issues: DecisionIssues,
    pub tags: Vec<String>,
    pub made_by_type: DecisionMakerType,
    #[serde(default)]
    pub made_by_user_id: Option<String>,
    #[serde(default)]
    pub made_by_agent_name: Option<String>,
    #[serde(default)]
    pub made_by_agent_rank: Option<AgentRank>,
    #[serde(default)]
    pub made_by_agent_model: Option<String>,
    #[serde(default)]
    pub evidence_kind: Option<DecisionEvidenceKind>,
    #[serde(default)]
    pub evidence_reference: Option<String>,
    #[serde(default)]
    pub evidence_excerpt: Option<String>,
    pub recorded_by_user_id: String,
    #[serde(default)]
    pub version: Option<u32>,
    pub revision_number: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl DecisionListItem {
    /// An unset API DateOnly is not a real decision date.
    // qual:api — consumed by the decision UI; tested here.
    pub fn decided_on(&self) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(&self.decided_on, "%Y-%m-%d")
            .ok()
            .filter(|date| Some(*date) != NaiveDate::from_ymd_opt(1, 1, 1))
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionDetail {
    #[serde(flatten)]
    pub item: DecisionListItem,
    pub decision: String,
    pub context: String,
    pub why: String,
    pub implications: String,
    pub alternatives: String,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<String>,
    #[serde(default)]
    pub accepted_by_user_id: Option<String>,
    #[serde(default)]
    pub rejected_by_user_id: Option<String>,
    pub provenance: DecisionProvenance,
    pub recorded_by: DecisionMember,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionSummary {
    pub id: String,
    pub title: String,
    pub status: DecisionStatus,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionScope {
    pub id: String,
    pub name: String,
    pub archived: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionIssues {
    pub items: Vec<DecisionIssue>,
    pub count: i32,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionProvenance {
    pub made_by_type: DecisionMakerType,
    #[serde(default)]
    pub made_by_user_id: Option<String>,
    #[serde(default)]
    pub made_by_agent_name: Option<String>,
    #[serde(default)]
    pub made_by_agent_rank: Option<AgentRank>,
    #[serde(default)]
    pub made_by_agent_model: Option<String>,
    #[serde(default)]
    pub evidence_kind: Option<DecisionEvidenceKind>,
    #[serde(default)]
    pub evidence_reference: Option<String>,
    #[serde(default)]
    pub evidence_excerpt: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DecisionMember {
    pub user_id: String,
    pub display_name: String,
    pub available: bool,
    pub kind: DecisionMakerType,
}
