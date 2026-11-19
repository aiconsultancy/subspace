//! Pi's JSONL RPC protocol behind Subspace's existing command/event contract.
use super::decisions::{ChatApproval, Decisions};
use super::export::{markdown_with_agent, write_export};
use super::names::{NEW_CHAT, clean_title, display_name, validate_name};
use super::tabs::{SavedTabs, read_tabs, save_tabs};
use super::{
    APPROVAL_TIMEOUT, Arc, Catalog, ChatCommand, ChatEvent, ChatId, Command, EVENT_LIMIT, Entry,
    EntryKind, Event, EventQueue, HashMap, MAX_CHATS, ModelChoice, Mutex, Path, PathBuf,
    SESSION_ID_LIMIT, ToolStatus, TranscriptEntry, Usage, mpsc, next_id, private_directory, state,
};
use crate::ChatPreferences;
use serde_json::{Value, json};
use std::fs;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use subspace_agent_tools::{Approval, CallMonitor, Invocation};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command as ProcessCommand};

#[path = "pi/process.rs"]
mod process;
#[cfg(test)]
use process::pi_process;
#[cfg(all(test, unix))]
use process::verify_pi_with_timeout;
use process::{find_pi, pi_command, verify_pi};

#[path = "pi/protocol.rs"]
mod protocol;
use super::refusal::{message_chat, refused, report_failed};
pub(super) use protocol::{command_chat, command_error, restored_active};
use protocol::{
    is_tool_request, message_entries, messages, model_choices, model_id, pi_usage, read_pi_history,
    remove_session_file, room_for_tab, stream_delta, tool_entry, tool_request, validate_session_id,
};

#[path = "pi/library.rs"]
pub(super) mod library;

#[path = "pi/chat_state.rs"]
mod chat_state;
use chat_state::{Models, NO_MODEL, Startup, Turn};
use library::{Library, LibraryItem, load as load_library, save as save_library};

const RPC_LIMIT: usize = super::child::RECORD_LIMIT;
const VERSION_OUTPUT_LIMIT: usize = 4 * 1024;
const STDERR_CHUNK: usize = 2 * 1024;
const PI_VERSION: &str = "1.0.4";
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
const THINKING_LEVELS: [&str; 5] = ["off", "minimal", "low", "medium", "high"];

type PiMessage = (ChatId, Incoming);

pub(super) async fn run(
    tools: Catalog,
    workspace: PathBuf,
    commands: mpsc::Receiver<Command>,
    events: Arc<EventQueue>,
    preferences: ChatPreferences,
) -> Result<(), String> {
    private_directory(&workspace)?;
    let executable = find_pi()?;
    verify_pi(&executable).await?;
    let bridge = write_bridge(&workspace, &tools)?;
    let sessions = workspace.join("sessions");
    private_directory(&sessions)?;
    let decisions = decisions(events.clone(), &workspace);
    PiActor::new(PiActorInit {
        tools,
        workspace,
        executable,
        bridge,
        sessions_dir: sessions,
        decisions,
        events,
    })?
    .listen(commands, preferences)
    .await
}

fn decisions(events: Arc<EventQueue>, workspace: &Path) -> Arc<Decisions> {
    Arc::new(Decisions {
        events,
        pending: Mutex::new(HashMap::new()),
        chats: Mutex::new(HashMap::new()),
        workspace: workspace.to_path_buf(),
    })
}

fn write_bridge(workspace: &Path, tools: &Catalog) -> Result<PathBuf, String> {
    let registrations = tools
        .tools()
        .iter()
        .map(|tool| bridge_registration(tool.spec()))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    let source =
        include_str!("../../assets/pi-bridge.ts").replace("/* SUBSPACE_TOOLS */", &registrations);
    let path = workspace.join("subspace-pi-bridge.ts");
    state::write_atomic(&path, &source)
        .map_err(|_| "Cannot stage the Pi tool bridge.".to_string())?;
    Ok(path)
}

fn bridge_registration(spec: &subspace_agent_tools::ToolSpec) -> Result<String, String> {
    let name = serde_json::to_string(spec.name).map_err(|_| "Cannot encode a Pi tool name.")?;
    let description = serde_json::to_string(spec.description)
        .map_err(|_| "Cannot encode a Pi tool description.")?;
    let label = serde_json::to_string(spec.title).map_err(|_| "Cannot encode a Pi tool label.")?;
    let schema = serde_json::to_string(&spec.input_schema())
        .map_err(|_| "Cannot encode a Pi tool schema.")?;
    Ok(format!(
        r#"  pi.registerTool({{
    name: {name}, label: {label}, description: {description}, parameters: {schema},
    async execute(_id: string, args: unknown, _signal: AbortSignal, _update: unknown, ctx: any) {{
      const reply = await ctx.ui.editor("subspace-tool:" + {name}, JSON.stringify(args));
      if (reply === undefined) throw new Error("Subspace cancelled the tool call.");
      const decoded = JSON.parse(reply);
      if (decoded.error) return {{ content: [{{ type: "text", text: decoded.error }}], details: undefined, isError: true }};
      return {{ content: [{{ type: "text", text: JSON.stringify(decoded.result) }}], details: undefined }};
    }}
  }});"#
    ))
}

struct PiActor {
    tools: Catalog,
    files: PiFiles,
    decisions: Arc<Decisions>,
    events: Arc<EventQueue>,
    chats: HashMap<ChatId, PiChat>,
    cancellations: HashMap<ChatId, Arc<AtomicBool>>,
    order: Vec<ChatId>,
    active: ChatId,
    library: Library,
    incoming: mpsc::Receiver<PiMessage>,
    sender: mpsc::Sender<PiMessage>,
}

struct PiFiles {
    workspace: PathBuf,
    executable: PathBuf,
    bridge: PathBuf,
    sessions_dir: PathBuf,
    tabs_file: PathBuf,
    library_file: PathBuf,
}

struct PiActorInit {
    tools: Catalog,
    workspace: PathBuf,
    executable: PathBuf,
    bridge: PathBuf,
    sessions_dir: PathBuf,
    decisions: Arc<Decisions>,
    events: Arc<EventQueue>,
}

impl PiActor {
    fn new(init: PiActorInit) -> Result<Self, String> {
        let PiActorInit {
            tools,
            workspace,
            executable,
            bridge,
            sessions_dir,
            decisions,
            events,
        } = init;
        let (sender, incoming) = mpsc::channel(EVENT_LIMIT);
        let tabs_file = state::sibling("pi-tabs.json")
            .ok_or_else(|| "Cannot find Pi tab storage.".to_string())?;
        let library_file = state::sibling("pi-library.json")
            .ok_or_else(|| "Cannot find Pi library storage.".to_string())?;
        let library = load_library(&library_file);
        Ok(Self {
            tools,
            files: PiFiles {
                workspace,
                executable,
                bridge,
                sessions_dir,
                tabs_file,
                library_file,
            },
            decisions,
            events,
            chats: HashMap::new(),
            cancellations: HashMap::new(),
            order: Vec::new(),
            active: 1,
            library,
            incoming,
            sender,
        })
    }

    async fn listen(
        mut self,
        mut commands: mpsc::Receiver<Command>,
        preferences: ChatPreferences,
    ) -> Result<(), String> {
        self.restore(&preferences).await?;
        let mut watchdog = tokio::time::interval(Duration::from_secs(1));
        loop {
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
            }
        }
        self.shutdown().await;
        Ok(())
    }

    async fn expire_startups(&mut self) {
        let expired: Vec<ChatId> = self
            .chats
            .iter()
            .filter(|(_, chat)| chat.startup.is_pending() && chat.started.elapsed() > RPC_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.process_exit(
                id,
                "Pi RPC startup timed out. Check Pi configuration and restart Subspace.",
            )
            .await;
        }
    }

    async fn restore(&mut self, preferences: &ChatPreferences) -> Result<(), String> {
        let saved = read_tabs(&self.files.tabs_file);
        let ids = saved.sessions.clone();
        let mut restored = Vec::new();
        for (index, session) in ids.iter().take(MAX_CHATS).enumerate() {
            let chat = u64::try_from(index + 1).unwrap_or(1);
            match self.open_chat(chat, Some(session.clone()), None).await {
                Ok(()) => restored.push((chat, session.clone())),
                Err(error) => self.events.push(Event::Error(error)).await,
            }
        }
        if restored.is_empty() {
            let chat = u64::try_from(ids.len() + 1).unwrap_or(1);
            self.open_chat(chat, None, Some(preferences.clone()))
                .await?;
            restored.push((chat, NEW_CHAT.to_owned()));
        }
        self.active = restored_active(&restored, saved.active.as_deref());
        self.events
            .push(Event::Restore {
                tabs: restored,
                active: self.active,
            })
            .await;
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
                .is_some_and(|chat| chat.turn.is_running())
        });
        report_failed(&self.events, chat, error, ended).await;
    }

    async fn apply(
        &mut self,
        command: Command,
        preferences: &ChatPreferences,
    ) -> Result<(), String> {
        match command {
            Command::Chat { chat, command } => self.chat_command(chat, command).await,
            Command::OpenChat {
                chat,
                session,
                preferences: requested,
            } => {
                let preferences =
                    requested.or_else(|| session.is_none().then(|| preferences.clone()));
                self.open_requested(chat, session, preferences).await
            }
            Command::OpenRemote { .. } => {
                Err("Pi does not support Copilot cloud-session replay.".into())
            }
            Command::Activate(chat) => self.activate(chat),
            Command::Approve { request_id, allow } => {
                self.decisions.approve(&request_id, allow);
                Ok(())
            }
            Command::Answer { .. } => {
                Err("Pi does not request Copilot user-input callbacks.".into())
            }
            Command::ListSessions => self.publish_library().await,
            Command::Refresh => self.refresh().await,
            Command::RenameSession { session, name } => self.rename_session(&session, &name).await,
            Command::DeleteSession(session) => self.delete_session(&session).await,
            Command::ExportSession { session, path } => self.export_session(&session, &path).await,
            Command::Shutdown => Ok(()),
        }
    }

    async fn chat_command(&mut self, id: ChatId, command: ChatCommand) -> Result<(), String> {
        if matches!(command, ChatCommand::Close) {
            return self.close_chat(id).await;
        }
        let chat = self
            .chats
            .get_mut(&id)
            .ok_or_else(|| "That chat is no longer open.".to_string())?;
        chat.apply(command, &self.events, &self.decisions).await?;
        self.active = id;
        self.save_tabs()
    }

    async fn open_requested(
        &mut self,
        id: ChatId,
        session: Option<String>,
        preferences: Option<ChatPreferences>,
    ) -> Result<(), String> {
        room_for_tab(self.chats.len())?;
        if let Some(saved) = session.as_deref() {
            validate_session_id(saved)?;
        }
        self.open_chat(id, session, preferences).await?;
        self.active = id;
        self.save_tabs()
    }

    async fn open_chat(
        &mut self,
        id: ChatId,
        saved: Option<String>,
        preferences: Option<ChatPreferences>,
    ) -> Result<(), String> {
        let title = saved
            .as_ref()
            .and_then(|id| self.library.items.get(id))
            .map(|item| item.title.as_str());
        let chat = PiChat::spawn(
            SpawnConfig {
                executable: &self.files.executable,
                workspace: &self.files.workspace,
                bridge: &self.files.bridge,
                sessions: &self.files.sessions_dir,
                saved: saved.as_deref(),
                title,
            },
            id,
            self.sender.clone(),
        )
        .await?;
        let mut chat = chat;
        if saved.is_none() {
            chat.preferred_model = preferences.and_then(|preferences| preferences.pi_model);
        }
        self.cancellations
            .insert(id, Arc::new(AtomicBool::new(false)));
        self.chats.insert(id, chat);
        self.order.retain(|open| *open != id);
        self.order.push(id);
        Ok(())
    }

    async fn close_chat(&mut self, id: ChatId) -> Result<(), String> {
        if let Some(process_exited) = self.cancellations.get(&id) {
            process_exited.store(true, Ordering::Release);
        }
        if let Some(chat) = self.chats.remove(&id) {
            chat.turn.cancel();
            self.decisions.cancel(id);
            chat.stop().await;
        }
        self.cancellations.remove(&id);
        self.order.retain(|open| *open != id);
        if self.active == id {
            self.active = self.order.last().copied().unwrap_or(id);
        }
        self.save_tabs()
    }

    async fn receive(&mut self, id: ChatId, incoming: Incoming) {
        match incoming {
            Incoming::Record(record) => self.receive_record(id, record).await,
            Incoming::ToolResult { request, result } => self.return_tool(id, request, result).await,
            Incoming::Exited(message) => self.process_exit(id, &message).await,
        }
    }

    async fn receive_record(&mut self, id: ChatId, record: Value) {
        if is_tool_request(&record) {
            self.call_tool(id, record);
            return;
        }
        let Some(chat) = self.chats.get_mut(&id) else {
            return;
        };
        let result = chat.receive(&record, &self.events).await;
        let startup_failed = result.is_err()
            && record["id"]
                .as_str()
                .is_some_and(|request| request.starts_with("startup-"));
        if let Err(error) = result {
            self.events
                .push(Event::of(id, ChatEvent::Error(error)))
                .await;
        }
        if startup_failed {
            self.process_exit(
                id,
                "Pi RPC startup failed. Correct the reported Pi error, then restart Subspace.",
            )
            .await;
            return;
        }
        self.update_library(id);
    }

    fn call_tool(&self, chat: ChatId, request: Value) {
        let Some((name, arguments)) = tool_request(&request) else {
            return;
        };
        let Some(tool) = self.tools.get(&name).cloned() else {
            let _ = self.sender.try_send((
                chat,
                Incoming::ToolResult {
                    request,
                    result: Err("Pi requested a tool outside the active board catalog.".into()),
                },
            ));
            return;
        };
        let sender = self.sender.clone();
        let approval: Arc<dyn Approval> = Arc::new(ChatApproval {
            decisions: self.decisions.clone(),
            chat,
        });
        let turn_cancelled = self
            .chats
            .get(&chat)
            .map(|open| open.turn.cancellation())
            .unwrap_or_default();
        let process_exited = self.cancellations.get(&chat).cloned().unwrap_or_default();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                APPROVAL_TIMEOUT,
                tool.call(Invocation {
                    arguments,
                    approval,
                    monitor: Arc::new(PiMonitor {
                        turn_cancelled,
                        process_exited,
                    }),
                }),
            )
            .await
            .map_err(|_| "The Subspace tool call timed out.".to_string())
            .and_then(|value| value);
            let _ = sender
                .send((chat, Incoming::ToolResult { request, result }))
                .await;
        });
    }

    async fn return_tool(&mut self, id: ChatId, request: Value, result: Result<Value, String>) {
        let Some(chat) = self.chats.get_mut(&id) else {
            return;
        };
        if let Err(error) = chat.extension_response(&request, result).await {
            self.events
                .push(Event::of(id, ChatEvent::Error(error)))
                .await;
        }
    }

    async fn process_exit(&mut self, id: ChatId, message: &str) {
        if let Some(cancelled) = self.cancellations.remove(&id) {
            cancelled.store(true, Ordering::Release);
        }
        self.decisions.cancel(id);
        if self.chats.remove(&id).is_some() {
            self.order.retain(|open| *open != id);
            self.events
                .push(Event::of(id, ChatEvent::Error(message.to_owned())))
                .await;
            self.events.push(Event::of(id, ChatEvent::TurnDone)).await;
            let _ = self.save_tabs();
        }
    }

    fn update_library(&mut self, id: ChatId) {
        let Some(chat) = self.chats.get(&id) else {
            return;
        };
        let Some(session) = chat.session_id.as_deref() else {
            return;
        };
        let item = LibraryItem {
            title: chat.title.clone(),
            path: chat.session_file.clone().unwrap_or_default(),
            modified_at: chrono::Utc::now().to_rfc3339(),
            created_at: chat.created_at.clone(),
        };
        self.library.items.insert(session.to_owned(), item);
        let _ = save_library(&self.library, &self.files.library_file);
    }

    fn activate(&mut self, chat: ChatId) -> Result<(), String> {
        if self.chats.contains_key(&chat) {
            self.active = chat;
            self.save_tabs()?;
        }
        Ok(())
    }

    async fn publish_library(&self) -> Result<(), String> {
        self.events
            .push(Event::Sessions(self.library.summaries()))
            .await;
        Ok(())
    }

    async fn refresh(&mut self) -> Result<(), String> {
        let Some(chat) = self.chats.get_mut(&self.active) else {
            return Ok(());
        };
        chat.command(json!({"type":"get_available_models","id":"refresh-models"}))
            .await
    }

    async fn rename_session(&mut self, session: &str, name: &str) -> Result<(), String> {
        validate_session_id(session)?;
        let name = validate_name(name)?;
        if let Some((_, chat)) = self
            .chats
            .iter_mut()
            .find(|(_, chat)| chat.session_id.as_deref() == Some(session))
        {
            chat.command(json!({"type":"set_session_name","id":"rename","name":name}))
                .await?;
        }
        let item = self
            .library
            .items
            .get_mut(session)
            .ok_or_else(|| "That Pi chat is not in Subspace's library.".to_string())?;
        item.title = name;
        save_library(&self.library, &self.files.library_file)?;
        self.publish_library().await
    }

    async fn delete_session(&mut self, session: &str) -> Result<(), String> {
        validate_session_id(session)?;
        if let Some(chat) = self
            .chats
            .iter()
            .find_map(|(id, chat)| (chat.session_id.as_deref() == Some(session)).then_some(*id))
        {
            self.close_chat(chat).await?;
        }
        let item = self
            .library
            .items
            .remove(session)
            .ok_or_else(|| "That Pi chat is not in Subspace's library.".to_string())?;
        remove_session_file(&item.path, &self.files.sessions_dir)?;
        save_library(&self.library, &self.files.library_file)?;
        self.publish_library().await
    }

    async fn export_session(&self, session: &str, path: &Path) -> Result<(), String> {
        validate_session_id(session)?;
        let item = self
            .library
            .items
            .get(session)
            .ok_or_else(|| "That Pi chat is not in Subspace's library.".to_string())?;
        let entries = read_pi_history(&item.path, &self.files.sessions_dir)?;
        write_export(path, &markdown_with_agent(&item.title, &entries, "Pi"))?;
        self.events
            .push(Event::Notice(format!(
                "Exported the chat to {}.",
                path.display()
            )))
            .await;
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
        save_tabs(&self.files.tabs_file, &SavedTabs { sessions, active })
    }

    async fn shutdown(&mut self) {
        self.decisions.cancel_all();
        for (_, chat) in self.chats.drain() {
            chat.stop().await;
        }
    }
}

struct SpawnConfig<'a> {
    executable: &'a Path,
    workspace: &'a Path,
    bridge: &'a Path,
    sessions: &'a Path,
    saved: Option<&'a str>,
    title: Option<&'a str>,
}

struct PiChat {
    id: ChatId,
    stdin: ChildStdin,
    child: Child,
    session_id: Option<String>,
    session_file: Option<String>,
    created_at: String,
    title: String,
    model: Option<String>,
    effort: Option<String>,
    models: Models,
    turn: Turn,
    started: Instant,
    startup: Startup,
    preferred_model: Option<String>,
    pending_model: Option<PendingModelConfirmation>,
}

struct PendingModelConfirmation {
    set_request_id: String,
    state_request_id: String,
    requested_model: String,
}

impl PiChat {
    async fn spawn(
        config: SpawnConfig<'_>,
        id: ChatId,
        sender: mpsc::Sender<PiMessage>,
    ) -> Result<Self, String> {
        let mut command = pi_command(&config);
        let mut child = command.spawn().map_err(|error| {
            format!("Pi could not start: {error}. Check the Pi executable and its Node runtime.")
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Pi did not open its RPC input.".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Pi did not open its RPC output.".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Pi did not open its diagnostic output.".to_string())?;
        super::child::forward_output(id, "Pi", stdout, stderr, sender);
        let mut chat = Self {
            id,
            stdin,
            child,
            session_id: config.saved.map(str::to_owned),
            session_file: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            title: config.title.unwrap_or(NEW_CHAT).to_owned(),
            model: None,
            effort: None,
            models: Models::Unknown,
            turn: Turn::Idle,
            started: Instant::now(),
            startup: Startup::Pending(0),
            preferred_model: None,
            pending_model: None,
        };
        chat.command(json!({"type":"get_state","id":"startup-state"}))
            .await?;
        chat.command(json!({"type":"get_available_models","id":"startup-models"}))
            .await?;
        chat.command(json!({"type":"get_messages","id":"startup-history"}))
            .await?;
        Ok(chat)
    }

    async fn apply(
        &mut self,
        command: ChatCommand,
        events: &EventQueue,
        decisions: &Decisions,
    ) -> Result<(), String> {
        match command {
            ChatCommand::CloseCanvas(_) | ChatCommand::OpenCanvas(_) => {
                Err("Native canvases require Copilot.".into())
            }
            ChatCommand::Send {
                text,
                context,
                images,
            } => {
                if !images.is_empty() {
                    return Err(
                        "Images require Copilot; Pi does not support image attachments.".into(),
                    );
                }
                self.send(text, context, events).await
            }
            ChatCommand::Abort => {
                self.turn.cancel();
                decisions.cancel(self.id);
                self.command(json!({"type":"clear_queue","id":"clear"}))
                    .await?;
                self.command(json!({"type":"abort","id":"abort"})).await
            }
            ChatCommand::SetModel { model, effort } => self.set_model(&model, effort, true).await,
            ChatCommand::Rename(name) => {
                let name = validate_name(&name)?;
                self.command(json!({"type":"set_session_name","id":"rename","name":name}))
                    .await
            }
            ChatCommand::SetAgent(_) => Err("Pi does not support Copilot custom agents.".into()),
            ChatCommand::SetMode(_) => Err("Pi does not support Copilot interaction modes.".into()),
            ChatCommand::SetYolo(_) => {
                Err("Pi native host tools are disabled; Copilot YOLO mode is unavailable.".into())
            }
            ChatCommand::RunCommand { .. } => {
                Err("Pi skills and prompt commands are disabled in Subspace.".into())
            }
            ChatCommand::Close => Ok(()),
        }
    }

    async fn send(
        &mut self,
        text: String,
        context: Vec<crate::ContextRef>,
        events: &EventQueue,
    ) -> Result<(), String> {
        self.models.require()?;
        if self.model.is_none() {
            return Err("Pi has no selected model. Choose one of Pi's configured models before sending work data.".into());
        }
        let message = super::startup::prepare_prompt(&text, &context, events)?;
        let id = format!("prompt-{}", next_id());
        let mut request = json!({"type":"prompt","id":id,"message":message});
        if self.turn.is_running() {
            request["streamingBehavior"] = json!("steer");
        }
        self.command(request).await?;
        self.turn.sent(id);
        if self.title == NEW_CHAT
            && let Some(title) = clean_title(&text)
        {
            self.title = title.clone();
            self.command(json!({"type":"set_session_name","id":"auto-name","name":title}))
                .await?;
        }
        Ok(())
    }

    async fn set_model(
        &mut self,
        model: &str,
        effort: Option<String>,
        persist: bool,
    ) -> Result<(), String> {
        let (provider, model_id) = model
            .split_once('/')
            .ok_or_else(|| "Choose a model Pi listed.".to_string())?;
        if let Some(level) = effort.as_deref()
            && !THINKING_LEVELS.contains(&level)
        {
            return Err("Pi does not support that thinking level in Subspace.".into());
        }
        let set_request_id = format!("set-model-{}", next_id());
        let state_request_id = format!("model-state-{}", next_id());
        self.command(
            json!({"type":"set_model","id":set_request_id,"provider":provider,"modelId":model_id}),
        )
        .await?;
        if let Some(level) = effort {
            self.command(json!({"type":"set_thinking_level","id":format!("set-thinking-{}", next_id()),"level":level}))
                .await?;
        }
        self.pending_model = persist.then_some(PendingModelConfirmation {
            set_request_id,
            state_request_id,
            requested_model: model.to_owned(),
        });
        Ok(())
    }

    async fn command(&mut self, value: Value) -> Result<(), String> {
        let mut encoded = serde_json::to_vec(&value)
            .map_err(|_| "Cannot encode a Pi RPC command.".to_string())?;
        if encoded.len() > RPC_LIMIT {
            return Err("Pi RPC command exceeds 1 MiB.".into());
        }
        encoded.push(b'\n');
        tokio::time::timeout(RPC_TIMEOUT, self.stdin.write_all(&encoded))
            .await
            .map_err(|_| "Pi RPC input timed out.".to_string())?
            .map_err(|_| "Pi RPC input closed unexpectedly.".to_string())
    }

    async fn extension_response(
        &mut self,
        request: &Value,
        result: Result<Value, String>,
    ) -> Result<(), String> {
        let id = request["id"]
            .as_str()
            .ok_or_else(|| "Pi tool bridge omitted its request id.".to_string())?;
        let payload = match result {
            Ok(value) => json!({"result":value}),
            Err(error) => json!({"error":error}),
        };
        self.command(json!({"type":"extension_ui_response","id":id,"value":payload.to_string()}))
            .await
    }

    async fn receive(&mut self, record: &Value, events: &EventQueue) -> Result<(), String> {
        match record["type"].as_str().unwrap_or_default() {
            "response" => self.response(record, events).await,
            "message_update" => {
                stream_delta(record, events, self.id);
                Ok(())
            }
            "message_end" => self.message_end(record, events).await,
            "tool_execution_start" | "tool_execution_end" => {
                events
                    .push(Event::of(self.id, ChatEvent::Entry(tool_entry(record))))
                    .await;
                Ok(())
            }
            "agent_settled" => {
                self.turn = Turn::Idle;
                events.push(Event::of(self.id, ChatEvent::TurnDone)).await;
                Ok(())
            }
            "session_info_changed" => self.title_changed(record, events).await,
            "thinking_level_changed" => {
                self.effort = record["level"].as_str().map(str::to_owned);
                self.publish_selection(events).await;
                Ok(())
            }
            "extension_error" => Err(format!(
                "Pi bridge error: {}",
                record["error"]
                    .as_str()
                    .unwrap_or("unknown extension error")
            )),
            _ => Ok(()),
        }
    }

    async fn response(&mut self, record: &Value, events: &EventQueue) -> Result<(), String> {
        if record["success"].as_bool() != Some(true) {
            let error = self.reject_response(record);
            if !self.turn.ran_nothing(record) {
                return Err(error);
            }
            refused(events, self.id, error).await;
            return Ok(());
        }
        self.handle_response(record, events).await?;
        if let Some(response) = startup_response(record["id"].as_str()) {
            self.startup = self.startup.received(response);
            self.publish_ready(events).await?;
        }
        Ok(())
    }

    fn reject_response(&mut self, record: &Value) -> String {
        let response_id = record["id"].as_str();
        if self.pending_model.as_ref().is_some_and(|pending| {
            Some(pending.set_request_id.as_str()) == response_id
                || Some(pending.state_request_id.as_str()) == response_id
        }) || record["command"].as_str() == Some("set_thinking_level")
        {
            self.pending_model = None;
        }
        format!(
            "Pi rejected {}: {}",
            record["command"].as_str().unwrap_or("a command"),
            record["error"].as_str().unwrap_or("no details")
        )
    }

    async fn handle_response(&mut self, record: &Value, events: &EventQueue) -> Result<(), String> {
        match record["command"].as_str().unwrap_or_default() {
            "get_state" => {
                self.state(&record["data"], events).await?;
                self.confirm_pending_model(record["id"].as_str(), events)
                    .await;
                Ok(())
            }
            "get_available_models" => self.models(&record["data"], events).await,
            "get_messages" => {
                events
                    .push(Event::of(
                        self.id,
                        ChatEvent::History(messages(&record["data"]["messages"])),
                    ))
                    .await;
                Ok(())
            }
            "set_model" => {
                let id = self
                    .pending_model
                    .as_ref()
                    .filter(|pending| {
                        Some(pending.set_request_id.as_str()) == record["id"].as_str()
                    })
                    .map(|pending| pending.state_request_id.clone())
                    .unwrap_or_else(|| format!("model-state-{}", next_id()));
                self.command(json!({"type":"get_state","id":id})).await
            }
            "set_thinking_level" => {
                self.command(
                    json!({"type":"get_state","id":format!("thinking-state-{}", next_id())}),
                )
                .await
            }
            "prompt" => {
                if self.turn.ran_nothing(record) {
                    events.push(Event::of(self.id, ChatEvent::TurnDone)).await;
                }
                Ok(())
            }
            "set_session_name" => Ok(()),
            _ => Ok(()),
        }
    }

    async fn confirm_pending_model(&mut self, state_id: Option<&str>, events: &EventQueue) {
        let Some(pending) = self.pending_model.as_ref() else {
            return;
        };
        if state_id != Some(pending.state_request_id.as_str())
            || self.model.as_deref() != Some(pending.requested_model.as_str())
        {
            return;
        }
        let Some(pending) = self.pending_model.take() else {
            return;
        };
        events
            .push(Event::of(
                self.id,
                ChatEvent::ModelConfirmed {
                    model: pending.requested_model,
                },
            ))
            .await;
    }

    async fn state(&mut self, data: &Value, events: &EventQueue) -> Result<(), String> {
        self.session_id = data["sessionId"]
            .as_str()
            .map(str::to_owned)
            .or(self.session_id.take());
        self.session_file = data["sessionFile"].as_str().map(str::to_owned);
        self.title = display_name(data["sessionName"].as_str(), None);
        self.model = model_id(&data["model"]).filter(|model| !model.contains("unknown"));
        self.effort = data["thinkingLevel"].as_str().map(str::to_owned);
        events
            .push(Event::of(self.id, ChatEvent::Title(self.title.clone())))
            .await;
        events
            .push(Event::of(self.id, ChatEvent::Mode(false)))
            .await;
        self.publish_selection(events).await;
        Ok(())
    }

    async fn publish_ready(&mut self, events: &EventQueue) -> Result<(), String> {
        if !self.startup.is_complete() {
            return Ok(());
        }
        let session_id = self
            .session_id
            .clone()
            .ok_or_else(|| "Pi did not report a session id.".to_string())?;
        self.startup = Startup::Ready;
        events
            .push(Event::of(self.id, ChatEvent::Ready { session_id }))
            .await;
        Ok(())
    }

    async fn models(&mut self, data: &Value, events: &EventQueue) -> Result<(), String> {
        let choices = model_choices(&data["models"]);
        let preferred = self.preferred_model.take();
        let preferred_available = preferred
            .as_deref()
            .is_some_and(|model| choices.iter().any(|choice| choice.id == model));
        self.models = Models::listed(&choices);
        events.push(Event::Models(choices)).await;
        events.push(Event::Agents(Vec::new())).await;
        events.push(Event::Commands(Vec::new())).await;
        if let Some(model) = preferred {
            if preferred_available {
                self.set_model(&model, None, false).await?;
            } else {
                events
                    .push(Event::Notice(format!(
                        "Saved Pi model '{model}' is unavailable; the configured fallback is active. The preference was kept."
                    )))
                    .await;
            }
        }
        if self.models == Models::None {
            events.push(Event::Error(NO_MODEL.into())).await;
        }
        Ok(())
    }

    async fn message_end(&mut self, record: &Value, events: &EventQueue) -> Result<(), String> {
        let message = &record["message"];
        for entry in message_entries(message, self.turn.steered()) {
            events
                .push(Event::of(self.id, ChatEvent::Entry(entry)))
                .await;
        }
        self.publish_assistant_end(message, events).await;
        Ok(())
    }

    async fn publish_assistant_end(&self, message: &Value, events: &EventQueue) {
        if message["role"].as_str() != Some("assistant") {
            return;
        }
        events
            .push(Event::of(
                self.id,
                ChatEvent::Usage(pi_usage(&message["usage"])),
            ))
            .await;
        if matches!(message["stopReason"].as_str(), Some("error" | "aborted")) {
            let error = message["errorMessage"]
                .as_str()
                .unwrap_or("Pi stopped the response.");
            events
                .push(Event::of(self.id, ChatEvent::Error(error.to_owned())))
                .await;
        }
    }

    async fn title_changed(&mut self, record: &Value, events: &EventQueue) -> Result<(), String> {
        self.title = display_name(record["name"].as_str(), None);
        events
            .push(Event::of(self.id, ChatEvent::Title(self.title.clone())))
            .await;
        Ok(())
    }

    async fn publish_selection(&self, events: &EventQueue) {
        if let Some(model) = &self.model {
            events
                .push(Event::of(
                    self.id,
                    ChatEvent::SelectedModel {
                        model: model.clone(),
                        effort: self.effort.clone(),
                    },
                ))
                .await;
        }
    }

    async fn stop(mut self) {
        self.turn.cancel();
        let _ = self.stdin.shutdown().await;
        if tokio::time::timeout(STOP_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
    }
}

fn startup_response(id: Option<&str>) -> Option<u8> {
    match id {
        Some("startup-state") => Some(0b001),
        Some("startup-models") => Some(0b010),
        Some("startup-history") => Some(0b100),
        _ => None,
    }
}

enum Incoming {
    Record(Value),
    ToolResult {
        request: Value,
        result: Result<Value, String>,
    },
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

struct PiMonitor {
    turn_cancelled: Arc<AtomicBool>,
    process_exited: Arc<AtomicBool>,
}

impl CallMonitor for PiMonitor {
    fn cancelled(&self) -> bool {
        self.turn_cancelled.load(Ordering::Acquire) || self.process_exited.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "pi/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pi/process_tests.rs"]
mod process_tests;

#[cfg(test)]
#[path = "pi/fixture_tests.rs"]
pub(super) mod fixture_tests;

#[cfg(all(test, unix))]
#[path = "pi/confirmation_tests.rs"]
mod confirmation_tests;

#[cfg(all(test, unix))]
#[path = "pi/startup_tests.rs"]
mod startup_tests;
