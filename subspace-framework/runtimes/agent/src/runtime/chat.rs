use super::chat_session::prepare_chat_session;
use super::names::{clean_title, display_name, first_message, validate_name};
use super::remote::{connect_replay, reject_read_only, saved_as_tab};
use super::session::{open_session, session_name, set_automatic_name, set_session_name, set_yolo};
use super::startup::{prepare_local_session, publish_history, publish_ready};
#[path = "chat_failure.rs"]
mod failing;
pub(super) use failing::end_reported;
#[path = "chat_forward.rs"]
mod forwarding;
#[path = "chat_held.rs"]
mod held;
use super::{
    ChatCommand, ChatEvent, ChatId, ChatPreferences, Client, Event, EventQueue, SessionId,
    SessionSetup, Transcript, bridge_event, commands::CommandOutcome, commands::invoke_command,
    display, mpsc, refresh, selection, validate_command,
};
use crate::failure::TurnContext;
use github_copilot_sdk::session::Session;
use github_copilot_sdk::subscription::{RecvError, RecvErrorKind};
use github_copilot_sdk::types::SessionEvent;
use std::time::Instant;
use tokio::task::JoinHandle;
pub(super) type SessionEventResult = Result<SessionEvent, RecvError>;
pub(super) type SessionMessage = (ChatId, SessionEventResult);
/// What every chat shares: the client, the session setup and the event queue.
pub(super) struct Env<'a> {
    pub(super) client: &'a Client,
    pub(super) setup: &'a SessionSetup,
    pub(super) events: &'a EventQueue,
}

pub(super) struct Chat {
    pub(super) id: ChatId,
    pub(super) session: Session,
    /// Copilot can resume a session only after it has a message, so a tab is
    /// saved from its first sent message.
    pub(super) persisted: bool,
    /// Maps SDK events to transcript rows; reset whenever history is replaced.
    transcript: Transcript,
    pub(super) busy: bool,
    pub(super) yolo: bool,
    pub(super) verified: bool,
    /// The session already has a name, so no automatic one is offered.
    named: bool,
    /// The chat replays a cloud agent session Copilot ran elsewhere. It
    /// accepts no command but closing, and it is never saved as a tab.
    read_only: bool,
    /// Settings chosen during the running turn.
    held: held::HeldSettings,
    /// The running turn and the chat's model, for failure reports.
    pub(super) turn: TurnContext,
    /// When the chat last received an SDK event, or the runtime last
    /// answered a ping for it.
    pub(super) last_event: Instant,
    /// The running turn's generation, fenced against the watchdog.
    pub(super) fence: failing::Generation,
    forwarder: JoinHandle<()>,
}

impl Chat {
    /// Opens `saved`, or a new chat, and publishes everything the tab shows.
    pub(super) async fn open(
        env: &Env<'_>,
        id: ChatId,
        saved: Option<SessionId>,
        sender: mpsc::Sender<SessionMessage>,
        preferences: Option<ChatPreferences>,
    ) -> Result<Self, String> {
        let persisted = saved.is_some();
        let session = open_session(env.client, saved, env.setup, id).await?;
        let yolo = prepare_chat_session(&session, env.setup, id, preferences.as_ref()).await?;
        env.setup.decisions.attach(session.id(), id);
        let forwarder = forwarding::forward(id, session.subscribe(), sender);
        let entries = publish_history(&session, id, env.events).await?;
        let name = session_name(&session).await;
        env.events
            .push(Event::of(
                id,
                ChatEvent::Title(display_name(name.as_deref(), first_message(&entries))),
            ))
            .await;
        publish_ready(&session, id, env.events).await;
        super::canvas::restore(&session, id, env.events).await;
        let selected = refresh(env.client, &session, id, env.events).await;
        let mut turn = TurnContext::default();
        turn.confirm(selected.as_ref());
        Ok(Self {
            id,
            session,
            persisted,
            transcript: Transcript::default(),
            busy: false,
            yolo,
            verified: true,
            named: name.is_some(),
            read_only: false,
            held: held::HeldSettings::default(),
            turn,
            last_event: Instant::now(),
            fence: Default::default(),
            forwarder,
        })
    }

    /// Replays a cloud agent task's last session. It connects to the task id
    /// and resumes what Copilot reports, with no workspace check, no tools
    /// and no permission handler: nothing in this session runs.
    pub(super) async fn open_replay(
        env: &Env<'_>,
        id: ChatId,
        task_id: &str,
        sender: mpsc::Sender<SessionMessage>,
    ) -> Result<Self, String> {
        let (session, title) = connect_replay(env.client, task_id).await?;
        let forwarder = forwarding::forward(id, session.subscribe(), sender);
        publish_history(&session, id, env.events).await?;
        env.events
            .push(Event::of(id, ChatEvent::Title(title)))
            .await;
        publish_ready(&session, id, env.events).await;
        Ok(Self {
            id,
            session,
            persisted: false,
            transcript: Transcript::default(),
            busy: false,
            yolo: false,
            verified: false,
            named: true,
            read_only: true,
            held: held::HeldSettings::default(),
            turn: TurnContext::default(),
            last_event: Instant::now(),
            fence: Default::default(),
            forwarder,
        })
    }

    /// Whether a restart reopens this chat's tab.
    pub(super) fn is_saved(&self) -> bool {
        saved_as_tab(self.persisted, self.read_only)
    }

    /// Detaches the session. It stays saved and keeps its name.
    pub(super) async fn close(self, setup: &SessionSetup) {
        super::lock(&setup.activity).remove(&self.id);
        setup.decisions.cancel(self.id);
        setup.decisions.detach(self.session.id());
        self.forwarder.abort();
        self.stop().await;
    }

    /// A replay only stops its event loop: it has no turn of its own to
    /// abort and no saved session to detach from.
    async fn stop(self) {
        if self.read_only {
            self.session.stop_event_loop().await;
            return;
        }
        let _ = self.session.abort().await;
        let _ = self.session.disconnect().await;
    }

    async fn apply_now(&mut self, env: &Env<'_>, command: ChatCommand) -> Result<(), String> {
        reject_read_only(&command, self.read_only)?;
        validate_command(&command, self.busy, self.verified)?;
        match command {
            ChatCommand::Send {
                text,
                context,
                images,
            } => self.send_message(env, text, context, images).await,
            ChatCommand::Abort => self.abort(env).await,
            ChatCommand::RunCommand { name, args } => self.run_command(env, &name, args).await,
            ChatCommand::SetYolo(enabled) => self.change_access(env, enabled).await,
            ChatCommand::Rename(name) => self.rename(env, name).await,
            setting => self.configure(env, setting).await,
        }
    }

    async fn configure(&mut self, env: &Env<'_>, command: ChatCommand) -> Result<(), String> {
        match command {
            ChatCommand::CloseCanvas(id) => super::canvas::close(&self.session, id).await,
            ChatCommand::OpenCanvas(canvas) => super::canvas::reopen(&self.session, &canvas).await,
            ChatCommand::SetModel { model, effort } => {
                let confirmed = selection::select_model(selection::ModelSelection {
                    session: &self.session,
                    events: env.events,
                    chat: self.id,
                    model: &model,
                    effort,
                    user_selected: true,
                })
                .await?;
                self.turn.confirm(Some(&confirmed));
                Ok(())
            }
            ChatCommand::SetAgent(name) => {
                selection::select_agent(&self.session, env.events, self.id, name).await
            }
            ChatCommand::SetMode(mode) => {
                selection::select_mode(&self.session, env.events, self.id, mode).await
            }
            _ => Ok(()),
        }
    }

    /// Only a listed skill runs. A skill that returns a prompt starts a turn;
    /// anything else is shown without sending, and starts no turn.
    async fn run_command(&mut self, env: &Env<'_>, name: &str, args: String) -> Result<(), String> {
        match invoke_command(&self.session, name, args).await? {
            CommandOutcome::Prompt(prompt) => {
                self.send_message(env, prompt, Vec::new(), Vec::new()).await
            }
            CommandOutcome::Note(note) => {
                display::note(env.events, self.id, note).await;
                super::refusal::ran_nothing(env.events, self.id, self.busy).await;
                Ok(())
            }
        }
    }

    /// Sends one message. The runtime picks the delivery mode from the chat's
    /// own state, which is the only place that knows whether a turn is
    /// running: a message typed during a turn steers it.
    /// After the first message, the SDK is offered the cleaned message as the
    /// chat's name, so the SDK and Subspace show the same title.
    pub(super) async fn name_automatically(&mut self, env: &Env<'_>, text: &str) {
        let Some(title) = clean_title(text) else {
            return;
        };
        if !set_automatic_name(&self.session, title.clone()).await {
            return;
        }
        self.named = true;
        env.events
            .push(Event::of(self.id, ChatEvent::Title(title)))
            .await;
    }

    async fn rename(&mut self, env: &Env<'_>, name: String) -> Result<(), String> {
        let name = validate_name(&name)?;
        set_session_name(&self.session, name.clone()).await?;
        self.named = true;
        env.events
            .push(Event::of(self.id, ChatEvent::Title(name)))
            .await;
        Ok(())
    }

    async fn abort(&mut self, env: &Env<'_>) -> Result<(), String> {
        env.setup.decisions.cancel(self.id);
        self.session
            .abort()
            .await
            .map_err(|_| "Copilot could not stop the current turn.".into())
    }

    async fn change_access(&mut self, env: &Env<'_>, enabled: bool) -> Result<(), String> {
        self.verified = false;
        let result = set_yolo(&self.session, enabled).await;
        let failure = result.err();
        self.yolo = enabled && failure.is_none();
        if let Some(error) = failure {
            self.verified = prepare_local_session(&self.session, &env.setup.workspace)
                .await
                .is_ok();
            env.events
                .push(Event::of(self.id, ChatEvent::Mode(false)))
                .await;
            return Err(error);
        }
        self.verified = true;
        env.events
            .push(Event::of(self.id, ChatEvent::Mode(enabled)))
            .await;
        Ok(())
    }

    /// Handles one SDK event and shares the chat's state with the watchdog.
    pub(super) async fn receive(
        &mut self,
        env: &Env<'_>,
        event: Result<SessionEvent, RecvError>,
    ) -> Result<(), String> {
        self.reconcile(env);
        let result = self.receive_now(env, event).await;
        self.publish(env);
        result
    }

    async fn receive_now(
        &mut self,
        env: &Env<'_>,
        event: Result<SessionEvent, RecvError>,
    ) -> Result<(), String> {
        match event {
            Ok(event) => self.received_event(env, event).await,
            Err(error) if matches!(error.kind(), RecvErrorKind::Lagged(_)) => {
                self.recover_stream(env).await
            }
            Err(_) => {
                self.stream_closed(env).await;
                Ok(())
            }
        }
    }

    async fn received_event(&mut self, env: &Env<'_>, event: SessionEvent) -> Result<(), String> {
        let value =
            serde_json::to_value(event).map_err(|_| "Cannot decode Copilot event.".to_string())?;
        self.last_event = Instant::now();
        self.turn.observe(&value);
        if !self.admit(env, value["type"].as_str().unwrap_or_default()) {
            return Ok(());
        }
        let failed = value["type"] == "session.error";
        let ended = failed || value["type"] == "session.idle";
        let failure = self.session_failed(env, failed, &value["data"]).await;
        self.busy &= !ended;
        bridge_event(self.id, &value, &mut self.transcript, env.events).await;
        self.recheck_policy(failed).await;
        if ended {
            self.end_turn(failure);
            self.apply_held(env).await;
        }
        Ok(())
    }

    async fn recover_stream(&mut self, env: &Env<'_>) -> Result<(), String> {
        self.abort(env).await?;
        publish_history(&self.session, self.id, env.events).await?;
        self.transcript = Transcript::default();
        self.busy = false;
        env.events.push(Event::of(self.id, ChatEvent::Error("Copilot's event stream fell behind. The turn was stopped and SDK history was restored.".into()))).await;
        env.events
            .push(Event::of(self.id, ChatEvent::TurnDone))
            .await;
        self.apply_held(env).await;
        Ok(())
    }
}
