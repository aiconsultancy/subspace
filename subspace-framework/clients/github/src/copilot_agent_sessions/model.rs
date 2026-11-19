//! The Copilot agent session document, `GET {api}/agents/sessions/{id}`.
//! The endpoint is undocumented, so every field is optional and keeps the
//! name the JSON uses.

use serde::{Deserialize, Serialize};

/// One Copilot agent session, as the Copilot API describes it. It carries
/// the fields the REST task detail does not: the pull request number and
/// state, the workflow run, and the prompt the session was given.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CopilotAgentSession {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub head_ref: String,
    #[serde(default)]
    pub base_ref: String,
    /// `pull` for a session that works on a pull request.
    #[serde(default)]
    pub resource_type: String,
    #[serde(default)]
    pub resource_number: Option<u32>,
    #[serde(default)]
    pub resource_state: String,
    #[serde(default)]
    pub workflow_run_id: Option<u64>,
    #[serde(default)]
    pub published_event_count: Option<u64>,
    #[serde(default)]
    pub remote_steerable: bool,
    #[serde(default)]
    pub error: Option<serde_json::Value>,
    #[serde(default)]
    pub agent_slug: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub last_updated_at: String,
    /// The prompt the session started from.
    #[serde(default)]
    pub event_content: String,
}
