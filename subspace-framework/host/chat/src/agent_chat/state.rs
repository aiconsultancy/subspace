use super::attention::TabUpdate;
use super::copilot_only::{self, CopilotOnly};
use super::logic::{agent_options, mode_option, model_options};
use super::sign_in::SignIn;
use super::tabs::{LastTurn, Queued, Tab, TabState};
use super::toolbar::ToolbarModel;
use super::{
    AgentHandle, ChatCommand, ChatId, ChatPanel, Command, ContextRef, Entry, Event, InputIntent,
    Pending, Submission, TranscriptEntry,
};

/// What a chat shows while it holds a message for a runtime that is still
/// starting, and the row it occupies until the chat's own history replaces
/// it.
const WAITING_ROW: &str = "waiting-for-chat-engine";

impl ChatPanel {
    pub(super) fn connected(&mut self, result: Result<AgentHandle, String>) {
        self.reset_tabs(Tab::new(1));
        match result {
            Ok(handle) => {
                self.engine = handle.engine();
                self.handle = Some(handle);
                self.error = None;
                self.errors.legacy = None;
            }
            Err(error) => {
                self.handle = None;
                self.set_error(error);
            }
        }
    }

    /// Starts a runtime with a fresh first chat. In a debug build, the
    /// failure fixture shows its own tab instead and starts nothing.
    pub(super) fn start(&mut self, cx: &mut super::Context<Self>) {
        if self.is_disabled() {
            return;
        }
        #[cfg(debug_assertions)]
        if let Some(fixture) = super::failure_view::fixture::from_env() {
            self.show_fixture(fixture);
            self.log_accepted_errors(cx);
            return;
        }
        let tools = self
            .providers
            .tools
            .as_ref()
            .and_then(|provider| provider(cx));
        self.connected(super::interaction::start_runtime(tools, cx));
        self.log_accepted_errors(cx);
    }

    /// Shows only `tab`, numbering the next chats after the restored range.
    fn reset_tabs(&mut self, tab: Tab) {
        self.tabs = vec![tab];
        self.active = 0;
        self.next_chat = super::FIRST_CHAT;
        // A new runtime reports a rejected token again if it still is one.
        self.sign_in = None;
    }

    /// Shows the debug failure fixture with no runtime and no startup error.
    #[cfg(debug_assertions)]
    pub(super) fn show_fixture(&mut self, fixture: super::failure_view::fixture::Fixture) {
        self.reset_tabs(fixture.tab());
        self.handle = None;
        self.error = None;
        self.errors.legacy = None;
    }

    pub(super) fn replace_current(&mut self, context: Option<ContextRef>) -> bool {
        let changed = self.current != context;
        self.current = context;
        changed
    }

    /// A command Subspace could not hand to the runtime is this chat's error.
    pub(super) fn command_result(&mut self, result: Result<(), String>) -> bool {
        if let Err(error) = result {
            self.tab_mut().set_error(error);
            return false;
        }
        true
    }

    /// Everything the composer toolbar shows, from the confirmed state.
    pub(super) fn toolbar_model(&self) -> ToolbarModel {
        let tab = self.tab();
        let capabilities = self.capabilities();
        ToolbarModel {
            copilot_controls: capabilities.copilot_controls,
            show_ai_units: capabilities.ai_units(),
            models: model_options(&self.models),
            model: tab.model.clone().map(Into::into),
            effort: tab.effort.clone().map(Into::into),
            mode: mode_option(tab.mode),
            approve_all: tab.yolo,
            agents: agent_options(&self.agents),
            agent: tab.agent.clone().map(Into::into),
            auto_context: self.auto_context,
            show_tools: self.show_tools,
            show_thinking: self.show_thinking,
            busy: tab.is_busy(),
            ready: tab.is_ready() && !tab.read_only,
            has_selection: self.current.is_some(),
            usage: tab.usage,
        }
    }

    pub(super) fn prepare_submission(
        &mut self,
        intent: InputIntent,
        context: Vec<ContextRef>,
    ) -> Submission {
        let mut submission = Submission {
            command: None,
            clear: true,
            starts_turn: false,
            steering: false,
            answer_id: None,
            last_turn: None,
        };
        match intent {
            InputIntent::AddContext => self.tab_mut().context = context,
            InputIntent::EnableYolo => {
                if self.capabilities().copilot_controls {
                    self.tab_mut().confirm_yolo = true;
                } else {
                    self.tab_mut()
                        .set_error(copilot_only::unavailable(CopilotOnly::Yolo).into());
                    submission.clear = false;
                }
            }
            InputIntent::DisableYolo => {
                if self.capabilities().copilot_controls {
                    self.tab_mut().confirm_yolo = false;
                    submission.command = Some(self.chat_command(ChatCommand::SetYolo(false)));
                } else {
                    self.tab_mut()
                        .set_error(copilot_only::unavailable(CopilotOnly::Yolo).into());
                    submission.clear = false;
                }
            }
            InputIntent::Rename(name) => {
                submission.command = Some(self.chat_command(ChatCommand::Rename(name)));
            }
            InputIntent::RunSkill { name, args } => {
                submission.starts_turn = true;
                submission.command =
                    Some(self.chat_command(ChatCommand::RunCommand { name, args }));
            }
            InputIntent::UnknownCommand => {
                let commands = if self.capabilities().copilot_controls {
                    "Commands: /new, /close, /rename <name>, /yolo on, /yolo off, /add-context, /sessions, /model, or a listed skill."
                } else {
                    "Commands: /new, /close, /rename <name>, /add-context, /sessions, or /model."
                };
                self.tab_mut().set_error(commands.into());
                submission.clear = false;
            }
            // The panel already handled these before the submission.
            InputIntent::NewChat
            | InputIntent::CloseChat
            | InputIntent::OpenSessions
            | InputIntent::OpenModels => {}
            InputIntent::Message(text) => self.message(&mut submission, text, context),
        }
        submission
    }

    /// A typed line answers an open question, steers the running turn, or
    /// starts a new one. A chat the runtime has not reported ready keeps the
    /// message until it does, so nothing typed while Copilot starts is lost.
    fn message(&mut self, submission: &mut Submission, text: String, context: Vec<ContextRef>) {
        let tab = self.tab();
        match tab.pending.first() {
            Some(Pending::Question { id, .. }) => {
                submission.answer_id = Some(id.clone());
                submission.command = Some(Command::Answer {
                    request_id: id.clone(),
                    answer: text,
                });
            }
            _ if !tab.is_ready() => self.queue(text, context),
            _ => {
                submission.starts_turn = true;
                submission.steering = tab.is_busy();
                let turn = LastTurn {
                    text,
                    context,
                    images: tab.images.clone(),
                };
                submission.command = Some(self.chat_command(turn.clone().into_command()));
                // Retry resends the message that started the turn. The SDK
                // reports the failed message by an id Subspace does not keep,
                // so a steering message never replaces the turn's first one.
                submission.last_turn = (!submission.steering).then_some(turn);
            }
        }
    }

    /// Holds a message until the chat is ready, and says so in the chat, so
    /// the message does not simply vanish from the input.
    fn queue(&mut self, text: String, context: Vec<ContextRef>) {
        let waiting = format!(
            "Waiting for {} to start. Your message goes as soon as it does.",
            self.engine_label()
        );
        let tab = self.tab_mut();
        tab.entries.push(TranscriptEntry {
            id: WAITING_ROW.to_owned(),
            entry: Entry::Info(waiting),
        });
        let images = tab.images.clone();
        tab.queued = Some(Queued {
            text,
            context,
            images,
        });
    }

    /// Addresses a command to the chat the panel shows.
    pub(super) fn chat_command(&self, command: ChatCommand) -> Command {
        Command::Chat {
            chat: self.tab().id,
            command,
        }
    }

    pub(super) fn finish_submission(&mut self, submission: Submission, accepted: bool) -> bool {
        if !accepted {
            return false;
        }
        if submission.starts_turn {
            self.tab_mut().begin();
        }
        if let Some(turn) = submission.last_turn {
            self.tab_mut().last_turn = Some(turn);
        }
        submission.clear
    }

    /// Routes an event to the tab it belongs to, and returns a transcript
    /// event for that tab.
    pub(super) fn apply_event(&mut self, event: Event) -> Option<(ChatId, TabUpdate)> {
        match event {
            Event::Error(error) => self.set_error(error),
            Event::SignInRequired(offer) => self.sign_in = Some(SignIn::offered(offer)),
            Event::Notice(notice) => self.view.notice = Some(notice),
            Event::Restore { .. } => {
                self.strip().receive(event);
                let carried = std::mem::take(&mut self.carried);
                super::carry::restore(&mut self.tabs, carried);
            }
            Event::Chat { .. } => return self.strip().receive(event),
            catalog => self.apply_catalog(catalog),
        }
        None
    }

    /// The tabs, the one shown and the next id, as one state the runtime's
    /// events act on.
    pub(super) fn strip(&mut self) -> TabState<'_> {
        TabState {
            tabs: &mut self.tabs,
            active: &mut self.active,
            next_chat: &mut self.next_chat,
        }
    }

    /// What the composer toolbar and the `/` list offer.
    fn apply_catalog(&mut self, event: Event) {
        match event {
            Event::Models(models) => self.models = models,
            Event::Agents(agents) => self.agents = agents,
            Event::Sessions(sessions) => self.sessions = sessions,
            Event::Commands(commands) => self.commands = commands,
            _ => {}
        }
    }
}
