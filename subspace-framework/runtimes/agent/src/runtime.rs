//! One SDK client on a dedicated runtime. The UI never waits for SDK startup.
#[path = "runtime/acp.rs"]
mod acp;
#[path = "runtime/child.rs"]
mod child;
#[path = "config.rs"]
mod config;
#[path = "runtime/persistence.rs"]
mod persistence;
#[path = "runtime/pi.rs"]
mod pi;
#[path = "runtime/prompts.rs"]
mod prompts;
#[path = "runtime/selection.rs"]
mod selection;
#[path = "runtime/startup.rs"]
mod startup;
use crate::ChatPreferences;
use persistence::parse_session_id;
#[path = "runtime/export.rs"]
mod export;
#[path = "runtime/library.rs"]
mod library;
pub use export::export_file_name;
#[path = "runtime/display.rs"]
mod display;
#[path = "runtime/names.rs"]
mod names;
#[path = "runtime/supervisor.rs"]
mod supervisor;
#[path = "runtime/tabs.rs"]
mod tabs;
#[path = "runtime/watchdog.rs"]
mod watchdog;
use display::{bridge_event, history_entries, publish_sessions, refresh};

#[path = "runtime/summary.rs"]
mod summary;
mod transcript;
use transcript::Transcript;
#[path = "runtime/builtins.rs"]
mod builtins;
#[path = "runtime/canvas.rs"]
mod canvas;
#[path = "runtime/canvas_negotiation.rs"]
mod canvas_negotiation;
#[path = "runtime/catalog.rs"]
mod catalog;
#[path = "runtime/chat.rs"]
mod chat;
#[path = "runtime/chat_session.rs"]
mod chat_session;
#[path = "runtime/choices.rs"]
mod choices;
#[path = "runtime/commands.rs"]
mod commands;
#[path = "runtime/evidence.rs"]
mod evidence;
#[path = "runtime/refusal.rs"]
mod refusal;
#[path = "runtime/remote.rs"]
mod remote;
use catalog::load_agents;
#[path = "runtime/sdk_tools.rs"]
mod sdk_tools;
#[path = "runtime/session.rs"]
mod session;
#[path = "runtime/setup.rs"]
mod setup;
use session::{
    configure_sandbox, ensure_managed_policy, reset_permissions, verify_identity,
    verify_local_session,
};
use startup::validate_command;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use github_copilot_sdk::rpc::{
    AgentSelectRequest, AgentsDiscoverRequest, EventLogReadRequest, EventsReadDirection,
    ModeSetRequest, NameSetAutoRequest, NameSetRequest, PermissionMode, PermissionsSetModeRequest,
    SandboxConfig, SandboxConfigAuth, SandboxConfigSource, SessionUpdateOptionsParams,
    SessionsReadPersistedEventsRequest, ToolsListRequest,
};
use github_copilot_sdk::session::Session;
use github_copilot_sdk::types::{
    CustomAgentConfig, ResumeSessionConfig, SessionConfig, SessionEvent, SessionId,
};
use github_copilot_sdk::{Client, ClientMode, ClientOptions, Transport};
use serde_json::Value;
use subspace_core::{
    Secret,
    config::{ChatEngine, Config},
    credentials::Credentials,
    features::{ACP_CHAT, FeatureStage, PI_CHAT},
    state,
};
use tokio::sync::mpsc;

use crate::{
    ATTACHMENT_MARKER, ChatCommand, ChatEvent, ChatId, ChatMode, Choice, Command, ContextRef,
    Entry, EntryKind, Event, MAX_CHATS, ModelChoice, SessionSummary, SlashCommand, ToolStatus,
    TranscriptEntry, Usage,
};
use actor::run;
use decisions::Decisions;
#[cfg(test)]
use files::copy_attachment;
use files::{contains_secret, copy_attachment_checked, private_directory, read_regular_file};
use interaction::EventQueue;
use setup::SessionSetup;
use subspace_agent_tools::{Catalog, ChatToolServer};

const EVENT_LIMIT: usize = 256;
const DELTA_LIMIT: usize = 64 * 1024;
const FILE_LIMIT: u64 = 2 * 1024 * 1024;
const WORKSPACE_LIMIT: u64 = 20 * 1024 * 1024;
const PROMPT_LIMIT: usize = 256 * 1024;
const APPROVAL_LIMIT: usize = 16;
const COMMAND_LIMIT: usize = 64;
const MAX_AGENT_FILES: usize = 64;
const SESSION_ID_LIMIT: usize = 128;
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

const CHAT_OFF: &str = "Chat is off. Choose an engine in Settings > Chat.";

/// Offline runtime boundary for consumer tests. No thread, process or service starts.
#[cfg(any(test, feature = "test-support"))]
impl AgentHandle {
    // qual:test_helper
    pub fn test_offline(
        workspace: PathBuf,
        events: Vec<Event>,
    ) -> (AgentHandle, mpsc::Receiver<Command>) {
        let (commands, receiver) = mpsc::channel(COMMAND_LIMIT);
        let queue = EventQueue::default();
        lock(&queue.items).extend(events);
        let handle = AgentHandle {
            inner: Arc::new(HandleInner {
                commands,
                events: Arc::new(queue),
                workspace,
                attachment_lock: Mutex::new(()),
                engine: ChatEngine::Copilot,
                _stop: supervisor::stop_pair().0,
            }),
        };
        (handle, receiver)
    }
}

pub(super) fn read_attachment_image(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    files::reject_sensitive_path(path, contains_secret)?;
    read_regular_file(path, limit, "image")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone)]
pub struct AgentHandle {
    inner: Arc<HandleInner>,
}

struct HandleInner {
    commands: mpsc::Sender<Command>,
    events: Arc<EventQueue>,
    workspace: PathBuf,
    attachment_lock: Mutex<()>,
    engine: ChatEngine,
    /// Dropped with the last handle, which stops the runtime even when it
    /// waits on a call that never returns (see `supervisor.rs`).
    _stop: supervisor::Stop,
}

impl AgentHandle {
    /// Starts chat with the agent tools of the product's selected boards.
    pub fn start(
        config: Arc<Config>,
        credentials: Arc<Credentials>,
        tools: Catalog,
    ) -> Result<Self, String> {
        Self::start_with_preferences(config, credentials, tools, ChatPreferences::default(), None)
    }

    /// `chat_tools` serves the app's tools to ACP agents, which speak MCP
    /// themselves; Copilot and Pi call `tools` directly.
    pub fn start_with_preferences(
        config: Arc<Config>,
        credentials: Arc<Credentials>,
        tools: Catalog,
        preferences: ChatPreferences,
        chat_tools: Option<Arc<dyn ChatToolServer>>,
    ) -> Result<Self, String> {
        if config.chat.engine == ChatEngine::Pi && !config.feature_enabled(&PI_CHAT) {
            return Err(pi_disabled_message(PI_CHAT.stage));
        }
        if config.chat.engine == ChatEngine::Acp && !config.feature_enabled(&ACP_CHAT) {
            return Err(acp_disabled_message(ACP_CHAT.stage));
        }
        let engine = config.chat.engine;
        let workspace_name = match engine {
            ChatEngine::Off => return Err(CHAT_OFF.into()),
            ChatEngine::Copilot => "copilot-workspace",
            ChatEngine::Pi => "pi-workspace",
            ChatEngine::Acp => "acp-workspace",
        };
        let workspace = state::sibling(workspace_name)
            .ok_or_else(|| "Cannot find the Subspace state directory.".to_string())?;
        let (commands, receiver) = mpsc::channel(COMMAND_LIMIT);
        let events = Arc::new(EventQueue::default());
        remember_secrets(&events, &config, engine);
        let (stop, stopped) = supervisor::stop_pair();
        launch_runtime(RuntimeLaunch {
            engine,
            acp: config.chat.acp.clone(),
            credentials,
            tools,
            chat_tools,
            workspace: workspace.clone(),
            receiver,
            events: events.clone(),
            preferences,
            stopped,
        })?;
        Ok(AgentHandle {
            inner: Arc::new(HandleInner {
                commands,
                events,
                workspace,
                attachment_lock: Mutex::new(()),
                engine,
                _stop: stop,
            }),
        })
    }

    pub fn send(&self, command: Command) -> Result<(), String> {
        if self.is_stopped() {
            return Err(stopped_message(
                self.engine().label(),
                self.inner.events.failure(),
            ));
        }
        self.inner.commands.try_send(command).map_err(|_| {
            format!(
                "{}'s command queue is full or closed. Try again.",
                self.engine().label()
            )
        })
    }

    pub fn take_events(&self) -> Vec<Event> {
        let events = lock(&self.inner.events.items).drain(..).collect();
        self.inner.events.space.notify_one();
        events
    }

    pub fn attach_file(&self, path: &Path) -> Result<ContextRef, String> {
        let _guard = lock(&self.inner.attachment_lock);
        copy_attachment_checked(path, &self.inner.workspace, |content| {
            self.inner.events.has_secret(content)
        })
    }

    /// Reads a dropped image through the same regular-file, symlink, path and
    /// open-race boundary as text attachments. The caller still validates the
    /// image format and decoded dimensions.
    pub fn read_attachment_image(&self, path: &Path, limit: u64) -> Result<Vec<u8>, String> {
        let _guard = lock(&self.inner.attachment_lock);
        files::reject_sensitive_path(path, |value| self.inner.events.has_secret(value))?;
        read_regular_file(path, limit, "image")
    }

    pub fn same_runtime(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn engine(&self) -> ChatEngine {
        self.inner.engine
    }

    /// Whether the runtime has stopped, after a startup failure or a
    /// shutdown. Reconnect starts a new one.
    pub fn is_stopped(&self) -> bool {
        self.inner.events.stopped.load(Ordering::Acquire)
    }
}

/// What a command to a stopped runtime says: the failure that stopped it,
/// which a later chat error would otherwise hide, else how to start again.
fn stopped_message(engine: &str, failure: Option<String>) -> String {
    match failure {
        Some(failure) => format!("{engine} is not running. {failure}"),
        None => format!("{engine} is not running. Click Reconnect to start it again."),
    }
}

fn pi_disabled_message(stage: FeatureStage) -> String {
    match stage {
        FeatureStage::Experimental => "Pi chat is experimental and disabled. Enable Pi chat in Settings > Experiments or choose another engine.",
        FeatureStage::Stable => "Pi chat is disabled. Enable Pi chat in Settings > General under regular features or choose another engine.",
        FeatureStage::Removed => "Pi chat is unavailable. Choose another engine in Settings > Chat.",
    }
    .into()
}

fn acp_disabled_message(stage: FeatureStage) -> String {
    match stage {
        FeatureStage::Experimental => "ACP agent chat is experimental and disabled. Enable ACP agent chat in Settings > Experiments or choose another engine.",
        FeatureStage::Stable => "ACP agent chat is disabled. Enable it in Settings > General or choose another engine.",
        FeatureStage::Removed => "ACP agent chat is unavailable. Choose another engine in Settings > Chat.",
    }
    .into()
}

fn remember_secrets(events: &EventQueue, config: &Config, engine: ChatEngine) {
    events.remember_secrets(config.github.token.clone());
    const SERVICE_SECRETS: [&str; 3] = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"];
    const PI_PROVIDER_SECRETS: [&str; 17] = [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_OAUTH_TOKEN",
        "OPENAI_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "DEEPSEEK_API_KEY",
        "GEMINI_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "OPENROUTER_API_KEY",
        "AI_GATEWAY_API_KEY",
        "ZAI_API_KEY",
        "MISTRAL_API_KEY",
        "OPENCODE_API_KEY",
        "KIMI_API_KEY",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
    ];
    events.remember_secrets(
        SERVICE_SECRETS
            .into_iter()
            .filter_map(|name| std::env::var(name).ok().map(Secret::new)),
    );
    if engine == ChatEngine::Acp {
        events.remember_secrets(
            config
                .chat
                .acp
                .env
                .iter()
                .filter(|(name, _)| {
                    ["KEY", "TOKEN", "SECRET", "PASSWORD"]
                        .iter()
                        .any(|marker| name.contains(marker))
                })
                .map(|(_, value)| Secret::new(value.clone())),
        );
    }
    if engine != ChatEngine::Copilot {
        events.remember_secrets(
            PI_PROVIDER_SECRETS
                .into_iter()
                .filter_map(|name| std::env::var(name).ok().map(Secret::new)),
        );
    }
}

struct RuntimeLaunch {
    engine: ChatEngine,
    acp: subspace_core::config::AcpConfig,
    credentials: Arc<Credentials>,
    tools: Catalog,
    chat_tools: Option<Arc<dyn ChatToolServer>>,
    workspace: PathBuf,
    receiver: mpsc::Receiver<Command>,
    events: Arc<EventQueue>,
    preferences: ChatPreferences,
    stopped: supervisor::Stopped,
}

struct RuntimeContext {
    commands: mpsc::Receiver<Command>,
    events: Arc<EventQueue>,
    preferences: ChatPreferences,
}

fn launch_runtime(launch: RuntimeLaunch) -> Result<(), String> {
    let engine = launch.engine;
    let thread_name = format!("subspace-{}", engine.label().to_ascii_lowercase());
    std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || run_engine(launch))
        .map_err(|_| format!("Cannot start the {} worker thread.", engine.label()))?;
    Ok(())
}

fn run_engine(launch: RuntimeLaunch) {
    let engine = launch.engine;
    let events = launch.events.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build();
    match runtime {
        Ok(runtime) => runtime.block_on(async {
            let result = match engine_work(launch) {
                Ok((work, stopped)) => {
                    supervisor::supervised(work, stopped, supervisor::SHUTDOWN_GRACE).await
                }
                Err(error) => Err(error),
            };
            if let Err(message) = result {
                events.fail(&message).await;
            }
        }),
        Err(_) => {
            events.push_initial_error(&format!("Cannot start the {} runtime.", engine.label()))
        }
    }
    events.stopped.store(true, Ordering::Release);
}

/// The selected engine's work, and the signal that stops it.
fn engine_work(launch: RuntimeLaunch) -> Result<(supervisor::Work, supervisor::Stopped), String> {
    let RuntimeLaunch {
        engine,
        acp,
        credentials,
        tools,
        chat_tools,
        workspace,
        receiver,
        events,
        preferences,
        stopped,
    } = launch;
    let work: supervisor::Work = match engine {
        ChatEngine::Off => return Err(CHAT_OFF.into()),
        ChatEngine::Copilot => Box::pin(run(
            credentials,
            tools,
            workspace,
            RuntimeContext {
                commands: receiver,
                events,
                preferences,
            },
        )),
        ChatEngine::Pi => Box::pin(pi::run(tools, workspace, receiver, events, preferences)),
        ChatEngine::Acp => Box::pin(acp::run(
            acp::AcpRun {
                config: acp,
                workspace,
                tools: chat_tools,
            },
            receiver,
            events,
            preferences,
        )),
    };
    Ok((work, stopped))
}

#[path = "runtime/decisions.rs"]
mod decisions;
#[path = "runtime/interaction.rs"]
mod interaction;
#[path = "runtime/permissions.rs"]
mod permissions;

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[path = "runtime/actor.rs"]
mod actor;
mod attachments;
#[cfg(test)]
mod batch_fixture;
#[cfg(test)]
mod replay_fixture;

#[path = "runtime/files.rs"]
mod files;

pub(crate) fn redact(text: &str) -> String {
    let mut result = text.to_owned();
    for prefix in ["github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        while let Some(start) = result.find(prefix) {
            let end = result[start..]
                .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .map_or(result.len(), |end| start + end);
            result.replace_range(start..end, "[redacted]");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::prompts::prompt;
    use super::{
        AgentHandle, EventQueue, contains_secret, copy_attachment, pi_disabled_message,
        verify_identity,
    };
    use crate::{ChatEvent, ContextRef, EntryKind, Event};
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use subspace_agent_tools::{Catalog, test_support};
    use subspace_core::{
        Secret,
        config::{ChatEngine, Config},
        features::FeatureStage,
    };

    fn offline_handle(workspace: std::path::PathBuf) -> super::AgentHandle {
        super::AgentHandle::test_offline(workspace, Vec::new()).0
    }

    fn decisions(events: Arc<EventQueue>) -> super::Decisions {
        super::Decisions {
            events,
            pending: Mutex::new(Default::default()),
            chats: Mutex::new(Default::default()),
            workspace: std::path::PathBuf::from("/no/such/workspace"),
        }
    }

    #[tokio::test]
    async fn handle_drains_events_and_copies_files_without_starting_sdk() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("reference.txt");
        fs::write(&source, "Reference evidence").unwrap();
        let handle = offline_handle(dir.path().join("workspace"));
        let reference = handle.attach_file(&source).unwrap();
        assert_eq!(reference.content, "Reference evidence");
        handle
            .inner
            .events
            .push(Event::of(1, ChatEvent::TurnDone))
            .await;
        assert!(matches!(
            handle.take_events().as_slice(),
            [Event::Chat {
                chat: 1,
                event: ChatEvent::TurnDone
            }]
        ));
        assert!(handle.take_events().is_empty());
    }

    #[tokio::test]
    async fn a_command_to_a_stopped_runtime_repeats_the_failure_that_stopped_it() {
        let dir = tempfile::tempdir().unwrap();
        let handle = offline_handle(dir.path().join("workspace"));
        handle
            .inner
            .events
            .fail("GitHub rejected the gh CLI token for octo-dev.")
            .await;
        handle
            .inner
            .events
            .stopped
            .store(true, std::sync::atomic::Ordering::Release);
        assert!(handle.is_stopped());
        assert_eq!(
            handle.send(super::Command::Refresh).unwrap_err(),
            "Copilot is not running. GitHub rejected the gh CLI token for octo-dev."
        );
        assert!(
            super::stopped_message("Copilot", None).contains("Click Reconnect"),
            "a runtime that shut down cleanly says how to start it again"
        );
    }

    #[test]
    fn authentication_does_not_fall_back_to_another_identity() {
        assert!(verify_identity(true, Some("work"), "WORK").is_ok());
        assert!(verify_identity(true, None, "work").is_ok());
        assert!(verify_identity(true, Some("personal"), "work").is_err());
        assert!(verify_identity(false, Some("work"), "work").is_err());
        assert!(verify_identity(false, None, "work").is_err());
    }

    #[test]
    fn default_chat_config_cannot_start_a_runtime() {
        let config = Config::default();
        let credentials = test_support::credentials(&config);
        let error = AgentHandle::start(Arc::new(config), credentials, Catalog::default())
            .err()
            .unwrap();
        assert_eq!(error, "Chat is off. Choose an engine in Settings > Chat.");
    }

    #[test]
    fn disabled_pi_is_rejected_before_workspace_or_process_startup() {
        let mut config = Config::default();
        config.chat.engine = ChatEngine::Pi;
        let credentials = test_support::credentials(&config);

        let error = AgentHandle::start(Arc::new(config), credentials, Catalog::default())
            .err()
            .unwrap();

        assert!(error.contains("Settings > Experiments"), "{error}");
        assert!(error.contains("choose another engine"), "{error}");
    }

    #[test]
    fn pi_gate_guidance_follows_the_feature_lifecycle() {
        let experimental = pi_disabled_message(FeatureStage::Experimental);
        let stable = pi_disabled_message(FeatureStage::Stable);
        let removed = pi_disabled_message(FeatureStage::Removed);

        assert!(experimental.contains("Settings > Experiments"));
        assert!(experimental.contains("experimental"));
        assert!(stable.contains("Settings > General"));
        assert!(stable.contains("regular features"));
        assert!(!stable.contains("Experiments"));
        assert!(removed.contains("unavailable"));
        assert!(removed.contains("Choose another engine"));
        assert!(!removed.contains("Enable Pi chat"));
    }

    #[test]
    fn attachments_are_private_copies_not_parent_directory_grants() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("notes.md");
        let workspace = dir.path().join("workspace");
        fs::write(&source, "Reference evidence").unwrap();
        let attached = copy_attachment(&source, &workspace).unwrap();
        assert_eq!(attached.content, "Reference evidence");
        let destination = attached.key.strip_prefix("file:").unwrap();
        assert!(std::path::Path::new(destination).starts_with(workspace.join("attachments")));
        assert_eq!(
            fs::read_to_string(destination).unwrap(),
            "Reference evidence"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(destination).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn credential_files_and_content_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        let source = dir.path().join("notes.md");
        fs::write(&source, "github_pat_do_not_disclose").unwrap();
        assert!(copy_attachment(&source, &workspace).is_err());
        let env = dir.path().join(".env.local");
        fs::write(&env, "arbitrary secret").unwrap();
        assert!(copy_attachment(&env, &workspace).is_err());
        assert!(!workspace.exists());
        assert!(contains_secret(concat!("-----BEGIN ", "PRIVATE KEY-----")));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_attachments_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("notes.md");
        fs::write(&source, "notes").unwrap();
        let link = dir.path().join("link.md");
        std::os::unix::fs::symlink(source, &link).unwrap();
        assert!(copy_attachment(&link, &dir.path().join("workspace")).is_err());
    }

    #[test]
    fn image_reads_reject_directories_sensitive_paths_and_oversize_files() {
        let dir = tempfile::tempdir().unwrap();
        let handle = offline_handle(dir.path().join("workspace"));
        assert!(handle.read_attachment_image(dir.path(), 16).is_err());
        let secret = dir.path().join(".env-image.png");
        fs::write(&secret, [1, 2, 3]).unwrap();
        assert!(handle.read_attachment_image(&secret, 16).is_err());
        let large = dir.path().join("large.png");
        fs::write(&large, [0; 17]).unwrap();
        assert!(handle.read_attachment_image(&large, 16).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn image_reads_reject_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("image.png");
        fs::write(&source, [1, 2, 3]).unwrap();
        let link = dir.path().join("linked.png");
        std::os::unix::fs::symlink(source, &link).unwrap();
        let handle = offline_handle(dir.path().join("workspace"));
        assert!(handle.read_attachment_image(&link, 16).is_err());
    }

    #[test]
    fn actual_read_stops_if_a_file_grows_after_inspection() {
        use std::io::Write as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("growing.png");
        fs::write(&path, [1, 2]).unwrap();
        let opened = fs::File::open(&path).unwrap();
        let inspected_len = opened.metadata().unwrap().len();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[3, 4, 5])
            .unwrap();
        let result = super::files::read_limited(opened, inspected_len, 4, "image");
        assert!(
            result.is_err(),
            "the fifth byte must be observed and rejected"
        );
    }

    #[test]
    fn context_is_deduplicated_and_bounded() {
        let item = ContextRef {
            key: "same".into(),
            label: "evidence".into(),
            content: "unique evidence".into(),
        };
        let text = prompt("question", &[item.clone(), item]).unwrap();
        assert_eq!(text.matches("unique evidence").count(), 1);
        assert!(prompt(&"x".repeat(256 * 1024 + 1), &[]).is_err());
    }

    #[test]
    fn only_the_sdk_session_id_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tabs.json");
        super::tabs::save_tabs(
            &file,
            &super::tabs::SavedTabs {
                sessions: vec!["abc-123".into()],
                active: Some("abc-123".into()),
            },
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            r#"{"sessions":["abc-123"],"active":"abc-123"}"#
        );
    }

    #[tokio::test]
    async fn flooded_deltas_cannot_displace_approvals_or_final_events() {
        let queue = EventQueue::default();
        for _ in 0..1000 {
            queue.delta(1, EntryKind::Assistant, "x ".repeat(512));
        }
        queue
            .push(Event::of(
                1,
                ChatEvent::Approval {
                    request_id: "one".into(),
                    summary: "exact write".into(),
                },
            ))
            .await;
        queue.push(Event::of(1, ChatEvent::TurnDone)).await;
        let entries = super::lock(&queue.items);
        assert!(entries.len() <= super::EVENT_LIMIT);
        assert!(entries.iter().any(|event| matches!(
            event,
            Event::Chat {
                event: ChatEvent::Approval { .. },
                ..
            }
        )));
        assert!(entries.iter().any(|event| matches!(
            event,
            Event::Chat {
                event: ChatEvent::TurnDone,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn one_chats_stream_never_mixes_into_another_tab() {
        let queue = EventQueue::default();
        queue.delta(1, EntryKind::Assistant, "first ".into());
        queue.delta(2, EntryKind::Assistant, "second ".into());
        queue.delta(1, EntryKind::Assistant, "again ".into());
        let entries = super::lock(&queue.items);
        let deltas: Vec<(u64, String)> = entries
            .iter()
            .filter_map(|event| match event {
                Event::Chat {
                    chat,
                    event: ChatEvent::Delta { text, .. },
                } => Some((*chat, text.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            deltas,
            vec![
                (1, "first ".to_owned()),
                (2, "second ".to_owned()),
                (1, "again ".to_owned())
            ]
        );
    }

    #[tokio::test]
    async fn approval_callback_waits_without_blocking_command_responses() {
        let events = Arc::new(EventQueue::default());
        let decisions = Arc::new(decisions(events.clone()));
        let callback = decisions.clone();
        let task = tokio::spawn(async move {
            callback
                .approval(
                    3,
                    "write-1".into(),
                    "Set work item 42 title to reviewed".into(),
                )
                .await
        });
        let request = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let event = super::lock(&events.items).pop_front();
                if let Some(Event::Chat {
                    chat,
                    event:
                        ChatEvent::Approval {
                            request_id,
                            summary,
                        },
                }) = event
                {
                    break (chat, request_id, summary);
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(request.0, 3, "the asking tab shows the approval");
        assert_eq!(request.2, "Set work item 42 title to reviewed");
        assert!(!task.is_finished());
        decisions.approve(&request.1, true);
        assert!(task.await.unwrap().unwrap());
        assert!(super::lock(&decisions.pending).is_empty());
        decisions.approve(&request.1, true);
        assert!(super::lock(&decisions.pending).is_empty());
    }

    #[tokio::test]
    async fn cancelling_one_chat_leaves_another_chats_request_pending() {
        let events = Arc::new(EventQueue::default());
        let decisions = Arc::new(decisions(events.clone()));
        let first = decisions.clone();
        let second = decisions.clone();
        let one = tokio::spawn(async move { first.approval(1, "a".into(), "one".into()).await });
        let two = tokio::spawn(async move { second.approval(2, "b".into(), "two".into()).await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while super::lock(&decisions.pending).len() < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        decisions.cancel(1);
        assert_eq!(super::lock(&decisions.pending).len(), 1);
        decisions.approve("b", true);
        assert!(two.await.unwrap().unwrap());
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), one)
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            "cancelling the first chat must resolve its approval as denied"
        );
    }

    #[tokio::test]
    async fn credential_like_operations_are_not_offered_for_approval() {
        let events = Arc::new(EventQueue::default());
        events.remember_secrets([Secret::new("opaque-service-credential")]);
        let decisions = decisions(events.clone());
        assert!(
            decisions
                .approval(
                    1,
                    "unsafe".into(),
                    "Post opaque-service-credential as a comment".into()
                )
                .await
                .is_err()
        );
        assert!(super::lock(&events.items).is_empty());
    }

    #[test]
    fn streaming_never_exposes_a_pat_split_across_chunks() {
        let events = EventQueue::default();
        events.delta(1, EntryKind::Assistant, "Result gh".into());
        events.delta(1, EntryKind::Assistant, "p_do_not_disclose".into());
        events.delta(1, EntryKind::Assistant, " done ".into());
        let text = super::lock(&events.items)
            .iter()
            .filter_map(|event| {
                if let Event::Chat {
                    event: ChatEvent::Delta { text, .. },
                    ..
                } = event
                {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<String>();
        assert!(!text.contains("do_not_disclose"));
        assert!(text.contains("[redacted]"));
    }
}
