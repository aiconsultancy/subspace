use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod environment;
pub use environment::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunFilters {
    pub branch: Option<String>,
    /// A workflow id, file name or path. When present, GitHub's per-workflow
    /// runs endpoint is used.
    pub workflow: Option<String>,
    pub status: Option<String>,
    pub actor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Actor {
    pub login: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub state: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct HeadCommit {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowRun {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub workflow_id: u64,
    #[serde(default)]
    pub run_number: u64,
    #[serde(default)]
    pub run_attempt: u64,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub head_branch: Option<String>,
    #[serde(default)]
    pub head_sha: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub run_started_at: String,
    #[serde(default)]
    pub actor: Option<Actor>,
    #[serde(default)]
    pub triggering_actor: Option<Actor>,
    #[serde(default)]
    pub head_commit: Option<HeadCommit>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Step {
    #[serde(default)]
    pub number: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Job {
    pub id: u64,
    #[serde(default)]
    pub run_id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub runner_name: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobLog {
    pub text: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DeploymentStatus {
    pub id: u64,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub environment_url: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Deployment {
    pub id: u64,
    #[serde(rename = "ref", default)]
    pub reference: String,
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(skip)]
    pub latest_status: Option<DeploymentStatus>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub id: u64,
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub author: Option<Actor>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TagCommit {
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TagRef {
    pub name: String,
    pub commit: TagCommit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Definition<T> {
    Readable(T),
    Unreadable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowDefinition {
    pub dispatch: Option<DispatchDefinition>,
    pub jobs: Vec<WorkflowJob>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DispatchDefinition {
    pub inputs: Vec<DispatchInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchInput {
    pub name: String,
    pub kind: DispatchInputType,
    pub required: bool,
    pub default: Option<String>,
    pub options: Vec<String>,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DispatchInputType {
    #[default]
    String,
    Boolean,
    Choice,
    Environment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowJob {
    pub key: String,
    pub name: Option<String>,
    pub needs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobDependency {
    pub job_id: u64,
    pub job_key: String,
    /// API job ids. A matrix dependency can expand to several predecessors.
    pub needs: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewState {
    Approved,
    Rejected,
}

pub struct DispatchRequest<'a> {
    pub repo: &'a str,
    pub workflow: &'a str,
    pub reference: &'a str,
    pub inputs: &'a BTreeMap<String, String>,
}

pub struct ReviewPendingRequest<'a> {
    pub repo: &'a str,
    pub run_id: u64,
    pub environment_ids: &'a [u64],
    pub state: ReviewState,
    pub comment: &'a str,
}

#[derive(Serialize)]
pub(crate) struct DispatchBody<'a> {
    #[serde(rename = "ref")]
    pub reference: &'a str,
    pub inputs: &'a BTreeMap<String, String>,
}
