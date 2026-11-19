//! What every chat session is created with, and where this account's open
//! tabs are kept.

use super::chat::Chat;
use super::names::NEW_CHAT;
use super::startup::session_file_name;
use super::tabs::{SavedTabs, migrate_single_chat, read_tabs, tabs_file_name};
use super::{Arc, ChatId, Client, CustomAgentConfig, Decisions, PathBuf, config, sdk_tools, state};
use subspace_agent_tools::Catalog;

pub(super) struct SessionSetup {
    pub(super) settings: config::CopilotConfig,
    pub(super) workspace: PathBuf,
    /// The verified GitHub login; each account keeps its own Copilot config.
    pub(super) login: String,
    pub(super) decisions: Arc<Decisions>,
    pub(super) agents: Vec<CustomAgentConfig>,
    /// The CLI's own tools, as discovered at startup.
    pub(super) builtin_tools: Vec<String>,
    /// The agent tools of the product's selected boards.
    pub(super) tools: Catalog,
    /// The Copilot runtime version `status.get` reported, if it answered.
    pub(super) runtime_version: Option<String>,
    /// What each chat last shared with the watchdog.
    pub(super) activity: super::watchdog::Activities,
}

impl SessionSetup {
    /// The board tools of one chat. Each chat gets its own approval
    /// callback, so a tool's approval reaches the tab that started the turn.
    pub(super) fn tools(&self, chat: ChatId) -> Vec<github_copilot_sdk::Tool> {
        sdk_tools::definitions(
            &self.tools,
            Arc::new(super::decisions::ChatApproval {
                decisions: self.decisions.clone(),
                chat,
            }),
        )
    }
}

/// The version of the running Copilot runtime, for failure reports. A
/// runtime that does not answer `status.get` leaves it unknown.
pub(super) async fn runtime_version(client: &Client) -> Option<String> {
    let version = client.get_status().await.ok().map(|status| status.version);
    let shown = version.as_deref().unwrap_or("unknown");
    subspace_core::log::Logger::new("agent").record(
        subspace_core::log::Level::Info,
        "copilot runtime started",
        &[
            ("runtime_version", shown),
            ("sdk_version", crate::failure::SDK_VERSION),
        ],
    );
    version
}

pub(super) fn chat_session(chat: Option<&Chat>) -> String {
    chat.map(|chat| chat.session.id().to_string())
        .unwrap_or_else(|| NEW_CHAT.to_owned())
}

pub(super) fn tabs_path(login: &str) -> Result<PathBuf, String> {
    state::sibling(&tabs_file_name(login))
        .ok_or_else(|| "Cannot find session state storage.".to_string())
}

/// The saved tabs of this account, migrating the single chat an earlier
/// Subspace saved.
pub(super) fn saved_tabs(login: &str) -> Result<SavedTabs, String> {
    let previous = state::sibling(&session_file_name(login))
        .ok_or_else(|| "Cannot find session state storage.".to_string())?;
    Ok(migrate_single_chat(
        read_tabs(&tabs_path(login)?),
        &previous,
    ))
}
