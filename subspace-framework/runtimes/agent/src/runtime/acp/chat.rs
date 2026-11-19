//! One chat: an ACP agent process and the one session it runs.
use super::mcp::SessionTools;
use super::process::{Launch, command};
use super::protocol::{
    self, Capabilities, Message, ModelOptions, RpcError, classify, model_options,
};
use super::transcript::{AcpTranscript, Output, fold};
use super::{
    AcpMessage, Arc, ChatEvent, ChatId, Event, EventQueue, HashMap, Incoming, Path, PathBuf,
    TranscriptEntry, Value, mpsc,
};
use crate::runtime::names::{NEW_CHAT, display_name, validate_name};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin};

const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) struct SpawnConfig<'a> {
    pub(super) launch: &'a Launch,
    pub(super) agent: &'static str,
    pub(super) cwd: &'a Path,
    pub(super) tools: SessionTools,
    pub(super) saved: Option<&'a str>,
    pub(super) title: Option<&'a str>,
}

/// What a sent request was for, so its answer can be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Pending {
    Initialize,
    Authenticate,
    NewSession,
    LoadSession,
    Prompt,
    SetModel(String),
    Config,
    RestoreSelection,
    Other,
}

/// One `session/request_permission` waiting for Subspace's approval card.
#[derive(Debug, Clone)]
pub(super) struct PermissionAsk {
    pub(super) request: Value,
    pub(super) generation: u64,
    pub(super) options: Value,
    pub(super) summary: String,
}

pub(super) struct AcpChat {
    pub(super) id: ChatId,
    /// Distinguishes output and approval callbacks from replaced processes.
    pub(super) process_id: u64,
    pub(super) agent: &'static str,
    pub(super) stdin: ChildStdin,
    pub(super) child: Child,
    pub(super) cwd: PathBuf,
    pub(super) tools: SessionTools,
    /// Subspace's context for the agent; see `context.rs`.
    pub(super) context: Option<Arc<str>>,
    /// Whether the context still has to lead the next prompt.
    pub(super) context_pending: bool,
    pub(super) next_request: u64,
    pub(super) pending: HashMap<u64, Pending>,
    pub(super) capabilities: Capabilities,
    pub(super) saved: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) title: String,
    pub(super) created_at: String,
    pub(super) ready: bool,
    pub(super) started: Instant,
    pub(super) cancelled: bool,
    pub(super) cancel_deadline: Option<tokio::time::Instant>,
    pub(super) permissions: Vec<(Value, u64)>,
    pub(super) recovering: bool,
    /// A message accepted while Stop replaces the agent process.
    pub(super) queued: Option<super::ChatCommand>,
    pub(super) entries: Vec<TranscriptEntry>,
    pub(super) preferred_model: Option<String>,
    pub(super) preferred_effort: Option<String>,
    pub(super) replaying: bool,
    pub(super) authenticated: bool,
    pub(super) busy: bool,
    pub(super) transcript: AcpTranscript,
    pub(super) models: ModelOptions,
}

impl AcpChat {
    pub(super) async fn spawn(
        config: SpawnConfig<'_>,
        id: ChatId,
        sender: mpsc::Sender<AcpMessage>,
    ) -> Result<Self, String> {
        let agent = config.agent;
        let mut child = command(config.launch, config.cwd)
            .spawn()
            .map_err(|error| format!("{agent} could not start: {error}."))?;
        let (stdin, stdout, stderr) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdin), Some(stdout), Some(stderr)) = (stdin, stdout, stderr) else {
            return Err(format!("{agent} did not open its standard streams."));
        };
        let process_id = super::next_id();
        crate::runtime::child::forward_output::<Incoming>(
            process_id, agent, stdout, stderr, sender,
        );
        let mut chat = Self {
            id,
            process_id,
            agent,
            stdin,
            child,
            cwd: config.cwd.to_path_buf(),
            context: config.tools.context(),
            tools: config.tools,
            context_pending: false,
            next_request: 0,
            pending: HashMap::new(),
            capabilities: Capabilities::default(),
            saved: config.saved.map(str::to_owned),
            session_id: None,
            title: config.title.unwrap_or(NEW_CHAT).to_owned(),
            created_at: chrono::Utc::now().to_rfc3339(),
            ready: false,
            started: Instant::now(),
            cancelled: false,
            cancel_deadline: None,
            permissions: Vec::new(),
            recovering: false,
            queued: None,
            entries: Vec::new(),
            preferred_model: None,
            preferred_effort: None,
            replaying: false,
            authenticated: false,
            busy: false,
            transcript: AcpTranscript::default(),
            models: ModelOptions::default(),
        };
        chat.request(
            Pending::Initialize,
            "initialize",
            protocol::initialize_params(),
        )
        .await?;
        Ok(chat)
    }

    pub(super) async fn request(
        &mut self,
        pending: Pending,
        method: &str,
        params: Value,
    ) -> Result<(), String> {
        self.next_request += 1;
        let id = self.next_request;
        self.pending.insert(id, pending);
        self.write(protocol::request(id, method, params)).await
    }

    pub(super) async fn write(&mut self, value: Value) -> Result<(), String> {
        let mut encoded = serde_json::to_vec(&value)
            .map_err(|_| format!("Cannot encode a message for {}.", self.agent))?;
        if encoded.len() > crate::runtime::child::RECORD_LIMIT {
            return Err(format!("A message for {} exceeds 1 MiB.", self.agent));
        }
        encoded.push(b'\n');
        let agent = self.agent;
        tokio::time::timeout(WRITE_TIMEOUT, self.stdin.write_all(&encoded))
            .await
            .map_err(|_| format!("{agent} stopped reading its input."))?
            .map_err(|_| format!("{agent}'s input closed unexpectedly."))
    }

    /// Reads one line from the agent. A permission request goes back to the
    /// actor, which asks Zack.
    pub(super) async fn receive(
        &mut self,
        record: &Value,
        events: &EventQueue,
    ) -> Result<Option<PermissionAsk>, String> {
        match classify(record) {
            Message::Response { id, result } => {
                let pending = self.pending.remove(&id).unwrap_or(Pending::Other);
                self.response(pending, result, events).await?;
                Ok(None)
            }
            Message::Request { id, method, params } => self.agent_request(id, method, params).await,
            Message::Notification {
                method: "session/update",
                params,
            } => {
                self.update(&params["update"], events).await;
                Ok(None)
            }
            Message::Notification { .. } | Message::Invalid => Ok(None),
        }
    }

    async fn response(
        &mut self,
        pending: Pending,
        result: Result<&Value, RpcError>,
        events: &EventQueue,
    ) -> Result<(), String> {
        match (pending, result) {
            (Pending::Initialize, Ok(result)) => self.initialized(result, events).await,
            (Pending::Authenticate, Ok(_)) => self.open_session().await,
            (pending @ (Pending::NewSession | Pending::LoadSession), Err(error))
                if error.needs_sign_in() =>
            {
                if self.authenticate(events).await? {
                    return Ok(());
                }
                Err(self.failure(&pending, &error))
            }
            (Pending::NewSession, Ok(result)) => self.session_opened(result, events).await,
            (Pending::LoadSession, Ok(result)) => self.session_loaded(result, events).await,
            (Pending::Prompt, result) => self.turn_ended(result, events).await,
            (Pending::SetModel(model), Ok(result)) => self.model_set(&model, result, events).await,
            (Pending::Config, Ok(result)) => self.config_set(result, events).await,
            (Pending::RestoreSelection, Ok(result)) => {
                self.config_set(result, events).await?;
                self.apply_preferred_model(events).await
            }
            (Pending::Other, Ok(_)) => Ok(()),
            (pending, Err(error)) => Err(self.failure(&pending, &error)),
        }
    }

    async fn update(&mut self, update: &Value, events: &EventQueue) {
        if self.recovering && self.replaying {
            return;
        }
        match update["sessionUpdate"].as_str().unwrap_or_default() {
            "session_info_update" => self.retitle(update["title"].as_str(), events).await,
            "available_commands_update" => {
                events
                    .push(Event::Commands(protocol::commands(update)))
                    .await;
            }
            "config_option_update" => {
                self.models = model_options(&update["configOptions"]);
                self.publish_models(events).await;
            }
            // The prompt response is authoritative. A trailing text chunk
            // must not make the host resume a turn the runtime has ended.
            _ if !self.busy && !self.replaying => {}
            _ => {
                let outputs = self.transcript.apply(update, self.replaying);
                if !self.replaying {
                    self.emit(outputs, events).await;
                } else {
                    fold(&mut self.entries, &outputs);
                }
            }
        }
    }

    pub(super) async fn retitle(&mut self, title: Option<&str>, events: &EventQueue) {
        let Some(title) = title.filter(|title| !title.trim().is_empty()) else {
            return;
        };
        self.title = display_name(Some(title), None);
        events
            .push(Event::of(self.id, ChatEvent::Title(self.title.clone())))
            .await;
    }

    pub(super) async fn emit(&mut self, outputs: Vec<Output>, events: &EventQueue) {
        fold(&mut self.entries, &outputs);
        for output in outputs {
            match output {
                Output::Delta(kind, text) => events.delta(self.id, kind, text),
                Output::Entry(entry) => {
                    events
                        .push(Event::of(self.id, ChatEvent::Entry(entry)))
                        .await;
                }
            }
        }
    }

    pub(super) async fn rename(&mut self, name: &str, events: &EventQueue) -> Result<(), String> {
        let name = validate_name(name)?;
        self.retitle(Some(&name), events).await;
        Ok(())
    }

    pub(super) async fn stop(mut self) {
        let _ = self.stdin.shutdown().await;
        if tokio::time::timeout(STOP_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
    }
}
