//! The public agent-task types and the raw documents they map from.

use serde::{Deserialize, Serialize};

use crate::model::PrState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubjectKind {
    /// A task the chat tool started; its subject id is the task id.
    Chat,
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub kind: SubjectKind,
    pub id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartTask {
    pub repo: String,
    pub prompt: String,
    pub model: Option<String>,
    pub base_ref: Option<String>,
    pub create_pull_request: bool,
    pub subject: Option<Subject>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestArtifact {
    pub number: u32,
    pub url: String,
    pub state: PrState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub repo: String,
    pub name: String,
    pub state: String,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
    pub pull_request: Option<PullRequestArtifact>,
    pub subject: Option<Subject>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskScope {
    pub configured_repos: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskList {
    pub tasks: Vec<Task>,
}

/// One agent run inside a task, from `GET /agents/tasks/{task_id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSession {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub head_ref: String,
    #[serde(default)]
    pub base_ref: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub usage: Option<SessionUsage>,
    #[serde(default)]
    pub error: Option<SessionError>,
}

/// What the session spent: `ai_credits` or `premium_requests`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUsage {
    #[serde(rename = "type")]
    pub kind: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionError {
    #[serde(default)]
    pub message: String,
}

/// A task with the sessions the detail endpoint adds.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskDetail {
    pub task: Task,
    pub sessions: Vec<TaskSession>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawSessions {
    #[serde(default)]
    pub(super) sessions: Vec<TaskSession>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawArtifact {
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) data: Option<RawArtifactData>,
}
#[derive(Debug, Deserialize)]
pub(super) struct RawArtifactData {
    pub(super) id: Option<u64>,
    pub(super) global_id: Option<String>,
}
#[derive(Debug, Deserialize)]
pub(super) struct RawPull {
    pub(super) id: u64,
    pub(super) number: u32,
    pub(super) html_url: String,
    pub(super) state: String,
    pub(super) draft: bool,
    pub(super) merged_at: Option<String>,
}
