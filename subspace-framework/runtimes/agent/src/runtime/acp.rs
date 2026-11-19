//! Agent Client Protocol agents (OpenCode, Claude Code, Codex, Gemini CLI)
//! behind Subspace's command/event contract. Each tab runs its own agent
//! process with one session, as Pi does; the agent speaks JSON-RPC 2.0 on its
//! standard streams. The running app's tools reach each session over its own
//! loopback MCP endpoint (see `mcp.rs`), and both their write approvals and
//! the agent's permission requests use the approval card.
use super::decisions::Decisions;
use super::export::{markdown_with_agent, write_export};
use super::names::{NEW_CHAT, validate_name};
use super::pi::library::{Library, LibraryItem, load as load_library, save as save_library};
use super::pi::{command_chat, restored_active};
use super::refusal::{message_chat, report_failed};
use super::tabs::{SavedTabs, read_tabs, save_tabs};
use super::{
    Arc, ChatCommand, ChatEvent, ChatId, Command, EVENT_LIMIT, Entry, EntryKind, Event, EventQueue,
    HashMap, MAX_CHATS, ModelChoice, Mutex, Path, PathBuf, SESSION_ID_LIMIT, SlashCommand,
    ToolStatus, TranscriptEntry, Usage, mpsc, next_id, private_directory, state,
};
use crate::ChatPreferences;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;
use subspace_agent_tools::ChatToolServer;
use subspace_core::config::AcpConfig;
use tokio::process::Command as ProcessCommand;

#[path = "acp/cancellation.rs"]
mod cancellation;
#[path = "acp/chat.rs"]
mod chat;
#[path = "acp/commands.rs"]
mod commands;
#[path = "acp/context.rs"]
mod context;
#[path = "acp/incoming.rs"]
mod incoming;
#[path = "acp/mcp.rs"]
mod mcp;
#[path = "acp/models.rs"]
mod models;
#[path = "acp/permissions.rs"]
mod permissions;
#[path = "acp/process.rs"]
mod process;
#[path = "acp/protocol.rs"]
mod protocol;
#[path = "acp/recovery.rs"]
mod recovery;
#[path = "acp/session_setup.rs"]
mod session_setup;
#[path = "acp/sessions.rs"]
mod sessions;
#[path = "acp/tabs.rs"]
mod tabs;
#[path = "acp/transcript.rs"]
mod transcript;
#[path = "acp/turns.rs"]
mod turns;

use chat::{AcpChat, PermissionAsk};
use process::Launch;

/// Startup includes `npx` fetching an adapter on first use.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);

type AcpMessage = (ChatId, Incoming);

enum Incoming {
    Record(Value),
    Permission { ask: PermissionAsk, allowed: bool },
    Exited(String),
}

impl super::child::Output for Incoming {
    fn record(record: Value) -> Self {
        Self::Record(record)
    }

    fn exited(message: String) -> Self {
        Self::Exited(message)
    }
}

/// What an ACP runtime starts from.
pub(super) struct AcpRun {
    pub(super) config: AcpConfig,
    pub(super) workspace: PathBuf,
    /// The app's tools, which each session receives over MCP.
    pub(super) tools: Option<Arc<dyn ChatToolServer>>,
}

pub(super) async fn run(
    run: AcpRun,
    commands: mpsc::Receiver<Command>,
    events: Arc<EventQueue>,
    preferences: ChatPreferences,
) -> Result<(), String> {
    let AcpRun {
        config,
        workspace,
        tools,
    } = run;
    private_directory(&workspace)?;
    let launch = process::resolve(&config, process::find)?;
    let agent = config.agent;
    let files = AcpFiles {
        tabs: storage(&format!("acp-{}-tabs.json", agent.id()))?,
        library: storage(&format!("acp-{}-library.json", agent.id()))?,
        workspace: workspace.clone(),
    };
    let decisions = decisions(events.clone(), &workspace);
    AcpActor::new(AcpActorInit {
        agent: agent.label(),
        launch,
        tools,
        files,
        decisions,
        events,
    })
    .listen(commands, preferences)
    .await
}

fn storage(name: &str) -> Result<PathBuf, String> {
    state::sibling(name).ok_or_else(|| "Cannot find ACP chat storage.".to_string())
}

fn decisions(events: Arc<EventQueue>, workspace: &Path) -> Arc<Decisions> {
    Arc::new(Decisions {
        events,
        pending: Mutex::new(HashMap::new()),
        chats: Mutex::new(HashMap::new()),
        workspace: workspace.to_path_buf(),
    })
}

struct AcpFiles {
    tabs: PathBuf,
    library: PathBuf,
    workspace: PathBuf,
}

struct AcpActorInit {
    agent: &'static str,
    launch: Launch,
    tools: Option<Arc<dyn ChatToolServer>>,
    files: AcpFiles,
    decisions: Arc<Decisions>,
    events: Arc<EventQueue>,
}

struct AcpActor {
    agent: &'static str,
    launch: Launch,
    tools: Option<Arc<dyn ChatToolServer>>,
    files: AcpFiles,
    decisions: Arc<Decisions>,
    events: Arc<EventQueue>,
    chats: HashMap<ChatId, AcpChat>,
    recoveries: HashMap<ChatId, recovery::Snapshot>,
    order: Vec<ChatId>,
    active: ChatId,
    library: Library,
    incoming: mpsc::Receiver<AcpMessage>,
    sender: mpsc::Sender<AcpMessage>,
}

impl AcpActor {
    fn new(init: AcpActorInit) -> Self {
        let (sender, incoming) = mpsc::channel(EVENT_LIMIT);
        let library = load_library(&init.files.library);
        Self {
            agent: init.agent,
            launch: init.launch,
            tools: init.tools,
            files: init.files,
            decisions: init.decisions,
            events: init.events,
            chats: HashMap::new(),
            recoveries: HashMap::new(),
            order: Vec::new(),
            active: 1,
            library,
            incoming,
            sender,
        }
    }

    async fn listen(
        mut self,
        mut commands: mpsc::Receiver<Command>,
        preferences: ChatPreferences,
    ) -> Result<(), String> {
        self.restore(&preferences).await?;
        let mut watchdog = tokio::time::interval(Duration::from_secs(1));
        loop {
            let cancellation = self
                .chats
                .values()
                .filter_map(|chat| chat.cancel_deadline)
                .min();
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break; };
                    if matches!(command, Command::Shutdown) { break; }
                    self.dispatch(command, &preferences).await;
                }
                message = self.incoming.recv() => {
                    let Some((chat, incoming)) = message else { break; };
                    self.receive(chat, incoming).await;
                }
                _ = watchdog.tick() => self.expire_startups().await,
                _ = cancellation::wait(cancellation) => self.expire_cancellations().await,
            }
        }
        self.shutdown().await;
        Ok(())
    }

    async fn expire_startups(&mut self) {
        let expired: Vec<ChatId> = self
            .chats
            .iter()
            .filter(|(_, chat)| !chat.ready && chat.started.elapsed() > STARTUP_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();
        let message = format!("{} did not start within two minutes.", self.agent);
        for id in expired {
            self.process_exit(id, &message).await;
        }
    }

    async fn restore(&mut self, preferences: &ChatPreferences) -> Result<(), String> {
        let saved = read_tabs(&self.files.tabs);
        let mut restored = Vec::new();
        for (index, session) in saved.sessions.iter().take(MAX_CHATS).enumerate() {
            let chat = u64::try_from(index + 1).unwrap_or(1);
            match self.open_chat(chat, Some(session.clone()), None).await {
                Ok(()) => restored.push((chat, session.clone())),
                Err(error) => self.events.push(Event::Error(error)).await,
            }
        }
        if restored.is_empty() {
            let chat = u64::try_from(saved.sessions.len() + 1).unwrap_or(1);
            self.open_chat(chat, None, Some(preferences.clone()))
                .await?;
            restored.push((chat, NEW_CHAT.to_owned()));
        }
        self.active = restored_active(&restored, saved.active.as_deref());
        let event = Event::Restore {
            tabs: restored,
            active: self.active,
        };
        self.events.push(event).await;
        self.save_tabs()
    }

    async fn dispatch(&mut self, command: Command, preferences: &ChatPreferences) {
        let chat = command_chat(&command);
        let message = message_chat(&command);
        let Err(error) = self.apply(command, preferences).await else {
            return;
        };
        let ended = message.filter(|id| {
            !self
                .chats
                .get(id)
                .is_some_and(|chat| chat.busy || chat.queued.is_some())
        });
        report_failed(&self.events, chat, error, ended).await;
    }

    fn activate(&mut self, chat: ChatId) -> Result<(), String> {
        if self.chats.contains_key(&chat) {
            self.active = chat;
            self.save_tabs()?;
        }
        Ok(())
    }

    async fn refresh(&mut self) -> Result<(), String> {
        if let Some(chat) = self.chats.get(&self.active) {
            chat.publish_models(&self.events).await;
        }
        Ok(())
    }

    fn save_tabs(&self) -> Result<(), String> {
        let sessions = self
            .order
            .iter()
            .filter_map(|id| self.chats.get(id)?.session_id.clone())
            .collect();
        let active = self
            .chats
            .get(&self.active)
            .and_then(|chat| chat.session_id.clone());
        save_tabs(&self.files.tabs, &SavedTabs { sessions, active })
    }

    async fn shutdown(&mut self) {
        self.decisions.cancel_all();
        for (_, chat) in self.chats.drain() {
            chat.stop().await;
        }
    }
}

#[cfg(test)]
#[path = "acp/tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(unix)]
#[path = "acp/fixture.rs"]
mod fixture;

#[cfg(test)]
#[cfg(unix)]
#[path = "acp/fixture_tests.rs"]
mod fixture_tests;

#[cfg(test)]
#[cfg(unix)]
#[path = "acp/context_tests.rs"]
mod context_tests;

#[cfg(all(test, unix))]
#[path = "acp/resilience_tests.rs"]
mod resilience_tests;

#[cfg(all(test, unix))]
#[path = "acp/review_tests.rs"]
mod review_tests;

#[cfg(all(test, unix))]
#[path = "acp/selection_tests.rs"]
mod selection_tests;

#[cfg(all(test, unix))]
#[path = "acp/listener_tests.rs"]
mod listener_tests;
