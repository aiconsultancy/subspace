use super::RuntimeContext;
use super::builtins::discover_builtin_tools;
use super::chat::{Chat, Env, SessionMessage};
use super::refusal::{message_chat, report_failed};
use super::session::prepare_account_directory;
use super::setup::{SessionSetup, runtime_version, saved_tabs, tabs_path};
use super::startup::{
    resolve_identity, room_for_a_tab, sandbox_preflight, start_client, validate_startup,
};
use super::tabs::{SavedTabs, save_tabs};
use super::{
    Arc, Catalog, ChatCommand, ChatEvent, ChatId, Client, Command, Credentials, Decisions,
    EVENT_LIMIT, Event, EventQueue, HashMap, Mutex, PathBuf,
};
use super::{config, load_agents, mpsc, parse_session_id, private_directory, publish_sessions};
use crate::ChatPreferences;

#[path = "actor/opening.rs"]
mod opening;

pub(super) async fn run(
    credentials: Arc<Credentials>,
    tools: Catalog,
    workspace: PathBuf,
    context: RuntimeContext,
) -> Result<(), String> {
    let RuntimeContext {
        commands,
        events,
        preferences,
    } = context;
    let mut actor = Actor::initialize(credentials, tools, workspace, events, preferences).await?;
    actor.listen(commands).await
}

/// The chat a command addresses, so a failure is reported in its own tab.
fn command_chat(command: &Command) -> Option<ChatId> {
    match command {
        Command::Chat { chat, .. }
        | Command::OpenChat { chat, .. }
        | Command::OpenRemote { chat, .. } => Some(*chat),
        _ => None,
    }
}

pub(super) struct Actor {
    pub(super) client: Client,
    pub(super) setup: SessionSetup,
    pub(super) chats: HashMap<ChatId, Chat>,
    /// The tab order, which is the order the chats were opened in.
    order: Vec<ChatId>,
    /// The chat that last started a turn or was opened; the tab a restart
    /// selects.
    pub(super) active: ChatId,
    tabs_file: PathBuf,
    pub(super) events: Arc<EventQueue>,
    sessions: mpsc::Sender<SessionMessage>,
    incoming: mpsc::Receiver<SessionMessage>,
    preferences: ChatPreferences,
}

impl Actor {
    async fn initialize(
        credentials: Arc<Credentials>,
        tools: Catalog,
        workspace: PathBuf,
        events: Arc<EventQueue>,
        preferences: ChatPreferences,
    ) -> Result<Self, String> {
        let settings = config::CopilotConfig::load()?;
        validate_startup(&settings, std::env::var_os("COPILOT_CLI_PATH").is_some())?;
        let identity = resolve_identity(&settings, credentials.clone(), &events).await?;
        events.remember_secrets([identity.token.clone()]);
        private_directory(&workspace)?;
        prepare_account_directory(&workspace, &identity.login)?;
        let client = start_client(&identity, &workspace).await?;
        let runtime_version = runtime_version(&client).await;
        sandbox_preflight(&client).await?;
        let decisions = Arc::new(Decisions {
            events: events.clone(),
            pending: Mutex::new(HashMap::new()),
            chats: Mutex::new(HashMap::new()),
            workspace: workspace
                .canonicalize()
                .unwrap_or_else(|_| workspace.clone()),
        });
        let agents = load_agents(&client, &settings, &workspace).await?;
        let builtin_tools = discover_builtin_tools(&client).await;
        let setup = SessionSetup {
            settings,
            workspace,
            login: identity.login.clone(),
            decisions,
            agents,
            builtin_tools,
            tools,
            runtime_version,
            activity: Default::default(),
        };
        let (sessions, incoming) = mpsc::channel(EVENT_LIMIT);
        let mut actor = Self {
            client,
            setup,
            chats: HashMap::new(),
            order: Vec::new(),
            active: 1,
            tabs_file: tabs_path(&identity.login)?,
            events,
            sessions,
            incoming,
            preferences,
        };
        actor.restore(saved_tabs(&identity.login)?).await?;
        Ok(actor)
    }

    async fn listen(&mut self, mut commands: mpsc::Receiver<Command>) -> Result<(), String> {
        // Stops before the shutdown calls, or when the actor is dropped.
        let watchdog = super::watchdog::start(self.client.clone(), &self.setup, &self.events);
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break; };
                    if matches!(command, Command::Shutdown) { break; }
                    self.dispatch(command).await;
                }
                message = self.incoming.recv() => {
                    let Some((chat, event)) = message else { break; };
                    self.receive(chat, event).await;
                }
            }
        }
        drop(watchdog);
        self.shutdown().await;
        Ok(())
    }

    async fn shutdown(&mut self) {
        self.setup.decisions.cancel_all();
        for chat in self.chats.drain().map(|(_, chat)| chat) {
            chat.close(&self.setup).await;
        }
    }

    async fn dispatch(&mut self, command: Command) {
        let chat = command_chat(&command);
        let message = message_chat(&command);
        let Err(error) = self.run(command).await else {
            return;
        };
        let ended = message.filter(|id| !self.chats.get(id).is_some_and(|chat| chat.busy));
        report_failed(&self.events, chat, error, ended).await;
    }

    async fn run(&mut self, command: Command) -> Result<(), String> {
        match command {
            Command::Approve { request_id, allow } => {
                self.setup.decisions.approve(&request_id, allow);
                Ok(())
            }
            Command::Answer { request_id, answer } => {
                self.setup.decisions.answer(&request_id, answer);
                Ok(())
            }
            Command::Chat { chat, command } => self.chat_command(chat, command).await,
            Command::OpenChat {
                chat,
                session,
                preferences,
            } => self.open_requested(chat, session, preferences).await,
            Command::OpenRemote { chat, task_id } => self.open_replay(chat, &task_id).await,
            Command::Activate(chat) => self.activate(chat),
            Command::RenameSession { session, name } => self.rename_session(session, name).await,
            Command::DeleteSession(session) => self.delete_session(session).await,
            Command::ExportSession { session, path } => self.export_session(session, path).await,
            Command::ListSessions => {
                publish_sessions(&self.client, &self.events).await;
                Ok(())
            }
            Command::Refresh => {
                super::display::refresh_models(&self.client, &self.events).await;
                Ok(())
            }
            Command::Shutdown => Ok(()),
        }
    }

    pub(super) async fn chat_command(
        &mut self,
        id: ChatId,
        command: ChatCommand,
    ) -> Result<(), String> {
        if matches!(command, ChatCommand::Close) {
            return self.close_chat(id).await;
        }
        let starts_turn = matches!(
            command,
            ChatCommand::Send { .. } | ChatCommand::RunCommand { .. }
        );
        let env = Env {
            client: &self.client,
            setup: &self.setup,
            events: &self.events,
        };
        let chat = self
            .chats
            .get_mut(&id)
            .ok_or_else(|| "That chat is no longer open.".to_string())?;
        chat.apply(&env, command).await?;
        if starts_turn {
            self.active = id;
            self.save_tabs()?;
        }
        Ok(())
    }

    /// Replays a cloud agent task in its own tab. The replay is not saved,
    /// so the tab list is left alone.
    async fn open_replay(&mut self, id: ChatId, task_id: &str) -> Result<(), String> {
        room_for_a_tab(self.chats.len())?;
        let chat = Chat::open_replay(&self.env(), id, task_id, self.sessions.clone()).await?;
        self.insert_chat(id, chat);
        self.active = id;
        Ok(())
    }

    /// What every chat is given: the client, the session setup and the queue.
    fn env(&self) -> Env<'_> {
        Env {
            client: &self.client,
            setup: &self.setup,
            events: &self.events,
        }
    }

    /// Puts an opened chat in the strip, at the end of the tab order.
    fn insert_chat(&mut self, id: ChatId, chat: Chat) {
        self.chats.insert(id, chat);
        self.order.retain(|open| *open != id);
        self.order.push(id);
    }

    /// Detaches a chat's session. Closing the last tab opens an empty one, so
    /// the panel always has a chat to show.
    pub(super) async fn close_chat(&mut self, id: ChatId) -> Result<(), String> {
        let Some(chat) = self.chats.remove(&id) else {
            return Ok(());
        };
        self.order.retain(|open| *open != id);
        chat.close(&self.setup).await;
        if self.active == id {
            self.active = self.order.last().copied().unwrap_or(id);
        }
        self.save_tabs()
    }

    async fn receive(&mut self, id: ChatId, event: super::chat::SessionEventResult) {
        let env = Env {
            client: &self.client,
            setup: &self.setup,
            events: &self.events,
        };
        let Some(chat) = self.chats.get_mut(&id) else {
            return;
        };
        let result = chat.receive(&env, event).await;
        if let Err(error) = result {
            self.events
                .push(Event::of(id, ChatEvent::Error(error)))
                .await;
        }
    }

    pub(super) fn save_tabs(&self) -> Result<(), String> {
        save_tabs(&self.tabs_file, &self.open_tabs())
    }

    /// The tabs a restart reopens: the chats that have a message, in order.
    fn open_tabs(&self) -> SavedTabs {
        let sessions: Vec<String> = self
            .order
            .iter()
            .filter_map(|id| self.chats.get(id))
            .filter(|chat| chat.is_saved())
            .map(|chat| chat.session.id().to_string())
            .collect();
        let active = self
            .chats
            .get(&self.active)
            .filter(|chat| chat.is_saved())
            .map(|chat| chat.session.id().to_string());
        SavedTabs { sessions, active }
    }
}

#[cfg(test)]
mod tests {
    use super::command_chat;
    use super::opening::{chat_id, restored_active};
    use crate::{ChatCommand, Command};

    #[test]
    fn restored_tabs_are_numbered_from_one_in_saved_order() {
        assert_eq!(chat_id(0), 1);
        assert_eq!(chat_id(2), 3);
    }

    /// With nothing saved, the runtime opens one fresh chat and numbers it
    /// first, so it lands on the tab the panel started with.
    #[test]
    fn a_start_with_nothing_saved_opens_the_first_chat() {
        let ids: Vec<&str> = Vec::new();
        assert_eq!(chat_id(ids.len()), 1);
    }

    /// When every saved chat fails to resume, the replacement is numbered
    /// past them, so the panel has to adopt a chat it has no tab for.
    #[test]
    fn a_replacement_chat_is_numbered_past_the_saved_ones() {
        assert_eq!(chat_id(1), 2, "one saved chat that did not resume");
        assert_eq!(chat_id(3), 4);
    }

    /// A saved chat that no longer resumes is dropped, so the saved active
    /// position can point past what reopened.
    #[test]
    fn the_restore_selects_the_saved_tab_or_the_first_that_reopened() {
        let restored = vec![(1, "one".to_owned()), (2, "two".to_owned())];
        assert_eq!(restored_active(&restored, 1), 2);
        assert_eq!(restored_active(&restored, 0), 1);
        assert_eq!(
            restored_active(&restored, 7),
            1,
            "a dropped chat falls back"
        );
        assert_eq!(restored_active(&[], 0), 1);
    }

    #[test]
    fn a_failure_is_reported_in_the_tab_that_asked_for_it() {
        assert_eq!(
            command_chat(&Command::Chat {
                chat: 4,
                command: ChatCommand::Abort
            }),
            Some(4)
        );
        assert_eq!(
            command_chat(&Command::OpenChat {
                chat: 7,
                session: None,
                preferences: None,
            }),
            Some(7)
        );
        assert_eq!(command_chat(&Command::ListSessions), None);
        assert_eq!(
            command_chat(&Command::OpenRemote {
                chat: 3,
                task_id: "3f0a".into()
            }),
            Some(3),
            "a failed replay is reported in the tab that asked for it"
        );
        assert_eq!(
            command_chat(&Command::Approve {
                request_id: "one".into(),
                allow: true
            }),
            None
        );
    }

    pub(super) fn replay_actor(
        peer: &super::super::replay_fixture::ReplayPeer,
        workspace: &std::path::Path,
    ) -> super::Actor {
        use super::super::{Catalog, Decisions, EventQueue, SessionSetup};
        use std::sync::{Arc, Mutex};
        let events = Arc::new(EventQueue::default());
        let (sessions, incoming) = tokio::sync::mpsc::channel(16);
        super::Actor {
            client: peer.client.clone(),
            setup: SessionSetup {
                settings: Default::default(),
                workspace: workspace.into(),
                login: "test-only".into(),
                decisions: Arc::new(Decisions {
                    events: events.clone(),
                    pending: Mutex::new(Default::default()),
                    chats: Mutex::new(Default::default()),
                    workspace: workspace.into(),
                }),
                agents: Vec::new(),
                builtin_tools: Vec::new(),
                tools: Catalog::new(Vec::new()).unwrap(),
                runtime_version: None,
                activity: Default::default(),
            },
            chats: Default::default(),
            order: Vec::new(),
            active: 1,
            tabs_file: workspace.join("tabs.json"),
            events,
            sessions,
            incoming,
            preferences: Default::default(),
        }
    }

    #[tokio::test]
    async fn replay_dispatch_rejects_mutations_before_transport_and_never_persists_the_tab() {
        use super::super::{lock, replay_fixture::ReplayPeer, tabs::read_tabs};
        let directory = tempfile::tempdir().unwrap();
        let peer = ReplayPeer::new(directory.path());
        let mut actor = replay_actor(&peer, directory.path());
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            actor.open_replay(4, "fixture-task"),
        )
        .await
        .unwrap()
        .unwrap();
        let loaded = lock(&peer.requests).len();
        for command in [
            ChatCommand::Send {
                text: "must not send".into(),
                context: Vec::new(),
                images: Vec::new(),
            },
            ChatCommand::RunCommand {
                name: "skill".into(),
                args: String::new(),
            },
            ChatCommand::Rename("must not rename".into()),
            ChatCommand::SetYolo(true),
            ChatCommand::Abort,
        ] {
            let error = actor.chat_command(4, command).await.unwrap_err();
            assert!(error.contains("read-only"), "{error}");
        }
        actor.chats.get_mut(&4).unwrap().persisted = true;
        actor.save_tabs().unwrap();
        let saved = read_tabs(&actor.tabs_file);
        assert!(saved.sessions.is_empty());
        assert!(saved.active.is_none());
        actor.close_chat(4).await.unwrap();
        assert!(actor.chats.is_empty());
        assert_eq!(
            lock(&peer.requests).len(),
            loaded,
            "rejected commands and closing a replay must send no mutation RPC"
        );
        let requests = lock(&peer.requests);
        let resume = requests
            .iter()
            .find(|request| request["method"] == "session.resume")
            .unwrap();
        assert_eq!(resume["params"]["availableTools"], serde_json::json!([]));
        assert!(resume["params"].get("workingDirectory").is_none());
    }
}

#[cfg(test)]
#[path = "actor/turn_tests.rs"]
mod turn_tests;
