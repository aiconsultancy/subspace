//! App-wide conversation. The actor survives hiding this view.
//! The reveal takes layout space; GPUI honors the platform reduced-motion setting.
//!
//! Every chat setting lives in the composer toolbar under the input, so the
//! panel itself shows only the transcript, approvals and errors.

mod attachments;
mod attention;
mod bridge;
mod canvas;
mod carry;
mod content;
mod copilot_only;
mod effects;
mod engine;
mod error_logging;
mod external;
mod failure_view;
mod ingestion;
mod interaction;
mod lifecycle;
mod logic;
mod rename;
mod sessions;
mod sign_in;
mod slash;
mod state;
mod status;
mod status_words;
mod strip;
mod tab_run;
mod tabbing;
mod tabs;
#[cfg(test)]
mod test_menu;
mod toolbar;
mod transcript;
mod view;

use interaction::{new_input, new_slash, new_toolbar, polling_task};
use logic::activity_label;
use sessions::SessionsView;
use sign_in::{SignIn, subscriptions as sign_in_subscriptions};
use tabs::Tab;

use std::sync::Arc;
use std::time::Duration;
use std::{cell::RefCell, rc::Rc};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Animation, App, AppContext as _, ClipboardItem, Context, Div, DragMoveEvent, ElementId,
    EmptyView, Entity, EventEmitter, PathPromptOptions, Render, ScrollHandle, SharedString,
    Stateful, Subscription, Task, Window, div, ease_out_quint, px, relative,
};
use subspace_agent::{
    AgentHandle, ChatCommand, ChatEngine, ChatEvent, ChatId, ChatMode, Choice, Command, ContextRef,
    Entry, EntryKind, Event, ModelChoice, SessionSummary, SlashCommand, ToolCall, ToolStatus,
    TranscriptEntry,
};

use crate::measure;
use crate::rich::{Format, rich_text};
use crate::{Tokens, prefs, services};

use crate::{input, shared};
use input::ChatInput;
use slash::SlashList;
use toolbar::ChatToolbar;

pub use attention::{ChatAttentionKind, ChatPanelEvent};

/// Offline runtime injection for shell integration tests only.
#[cfg(feature = "test-support")]
pub mod test_support;

pub const OPEN_KEY: &str = "chat.open";
pub type ContextProvider = Rc<dyn Fn(&App) -> Option<ContextRef>>;
/// The app's tools for ACP sessions, read when a runtime starts.
pub type ChatToolsProvider = Rc<dyn Fn(&App) -> Option<Arc<dyn subspace_agent::ChatToolServer>>>;

/// What the shell supplies: the visible board's reference and the tools
/// ACP sessions receive.
#[derive(Default)]
struct Providers {
    context: Option<ContextProvider>,
    tools: Option<ChatToolsProvider>,
}
const WIDTH_KEY: &str = "chat.width";
const POLL_INTERVAL: Duration = Duration::from_millis(75);
const REVEAL_DURATION: Duration = Duration::from_millis(180);
const DEFAULT_WIDTH: f32 = 420.;
const DEFAULT_CANVAS_WIDTH: f32 = 380.;
/// Leave a title/tab lane beside header actions; their measured row overflows.
const HEADER_ACTION_MAX_SHARE: f32 = 0.75;
/// The first chat id the panel assigns itself. The runtime numbers the tabs
/// it restores from 1, so the panel's own ids start past that range and never
/// collide with them.
const FIRST_CHAT: ChatId = subspace_agent::MAX_CHATS as ChatId + 1;
const MIN_WIDTH: f32 = 300.;
const MAX_WIDTH_FRACTION: f32 = 0.6;

const PENDING_HEIGHT: f32 = 240.;
const HANDLE_LEFT: f32 = -3.;
const HANDLE_WIDTH: f32 = 6.;
const FULL_FRACTION: f32 = 1.;
const YOLO_WARNING: &str = concat!(
    "YOLO grants full host access and permits service writes and shell commands without ordinary approval. ",
    "Enterprise restrictions still apply. Enable for this conversation?"
);
/// What a read-only cloud replay says above its disabled input. The SDK
/// reports the events at load time, so the transcript carries no agent times.
const REPLAY_BANNER: &str = "Replay of the last cloud agent session; times are from loading";

struct ChatResize;

#[derive(Clone)]
enum Pending {
    Approval {
        id: String,
        summary: String,
    },
    Question {
        id: String,
        text: String,
        choices: Vec<String>,
    },
}

/// What a submitted line means. Slash commands never reach the runtime as
/// text; a skill runs through `RunCommand`.
#[derive(Debug, PartialEq, Eq)]
enum InputIntent {
    AddContext,
    EnableYolo,
    DisableYolo,
    NewChat,
    CloseChat,
    Rename(String),
    OpenSessions,
    OpenModels,
    RunSkill { name: String, args: String },
    UnknownCommand,
    Message(String),
}

struct Submission {
    command: Option<Command>,
    clear: bool,
    starts_turn: bool,
    /// The message joins a turn that is already running, so the turn's timer
    /// and status phrase keep going instead of restarting.
    steering: bool,
    answer_id: Option<String>,
    /// The message this submission sends, which Retry can send again.
    last_turn: Option<tabs::LastTurn>,
}

#[derive(Clone, Copy)]
enum Preference {
    Tools,
    Thinking,
    AutoContext,
}

pub struct ChatPanel {
    engine: ChatEngine,
    /// The config file selects Copilot, which this build does not include:
    /// the panel is off and says why, and the file keeps the choice.
    copilot_kept: bool,
    /// The configured ACP agent, which names the engine when it is ACP.
    acp_agent: subspace_agent::AcpAgent,
    handle: Option<AgentHandle>,
    input: Entity<ChatInput>,
    toolbar: Entity<ChatToolbar>,
    slash: Entity<SlashList>,
    /// The open chats, in strip order, and the one shown.
    tabs: Vec<Tab>,
    active: usize,
    /// The next chat id the panel hands to the runtime.
    next_chat: ChatId,
    /// The tab whose title is being edited, if any.
    renaming: Option<ChatId>,
    rename_input: Entity<InputState>,
    /// How wide the tab strip was last laid out, so the tabs can share it.
    strip_width: Entity<f32>,
    header_toolbar: RefCell<Option<Entity<subspace_dashboard::overflow_toolbar::OverflowToolbar>>>,
    canvas_toolbar: RefCell<Option<Entity<subspace_dashboard::overflow_toolbar::OverflowToolbar>>>,
    /// The Sessions tab replaces the transcript and the composer while it is
    /// selected; the chat tabs keep their state behind it.
    sessions_open: bool,
    view: SessionsView,
    current: Option<ContextRef>,
    providers: Providers,
    auto_context: bool,
    show_tools: bool,
    show_thinking: bool,
    models: Vec<ModelChoice>,
    agents: Vec<Choice>,
    sessions: Vec<SessionSummary>,
    commands: Vec<SlashCommand>,
    /// A failure that belongs to no single chat, such as a startup error.
    error: Option<String>,
    errors: error_logging::AcceptedErrors,
    /// The sign-in that would give chat a working GitHub account.
    sign_in: Option<SignIn>,
    /// Failures a reconnect keeps until the new runtime restores their chats.
    carried: Vec<carry::Carried>,
    attachment_ingestion_busy: bool,
    width: f32,
    canvas_width: f32,
    canvas_focus: gpui_kit::FocusHandle,
    reveal: u64,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
    /// Follows the shared Sign in with GitHub.
    _sign_in: Vec<Subscription>,
    _poller: Task<()>,
    _shutdown: Subscription,
}

impl EventEmitter<ChatPanelEvent> for ChatPanel {}

impl ChatPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (engine, copilot_kept) =
            engine::runnable(configured_engine(cx), subspace_agent::COPILOT_BUILT_IN);
        Self {
            copilot_kept,
            ..Self::with_engine(engine, window, cx)
        }
    }

    fn with_engine(engine: ChatEngine, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let toolbar = new_toolbar(window, cx);
        let input = new_input(engine, configured_acp_agent(cx), &toolbar, window, cx);
        let slash = new_slash(engine, window, cx);
        let rename_input = cx.new(|cx| InputState::new(window, cx));
        let session_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let session_rename = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&input, window, Self::on_input_event),
            cx.subscribe_in(&toolbar, window, Self::on_toolbar_event),
            cx.subscribe_in(&slash, window, Self::on_slash_event),
            cx.subscribe_in(&rename_input, window, Self::on_rename_event),
            cx.subscribe_in(&session_search, window, Self::on_search_event),
            cx.subscribe_in(&session_rename, window, Self::on_session_rename_event),
        ];
        Self {
            engine,
            copilot_kept: false,
            acp_agent: configured_acp_agent(cx),
            handle: None,
            input,
            toolbar,
            slash,
            tabs: vec![Tab::new(1)],
            active: 0,
            next_chat: FIRST_CHAT,
            renaming: None,
            rename_input,
            strip_width: measure::unmeasured(cx),
            header_toolbar: RefCell::new(None),
            canvas_toolbar: RefCell::new(None),
            sessions_open: false,
            view: SessionsView::new(session_search, session_rename, measure::unmeasured(cx)),
            current: None,
            providers: Providers::default(),
            auto_context: prefs::get(cx, "chat.auto-context").as_deref() != Some("0"),
            show_tools: prefs::get(cx, "chat.tools").as_deref() != Some("0"),
            show_thinking: prefs::get(cx, "chat.thinking").as_deref() == Some("1"),
            models: Vec::new(),
            agents: Vec::new(),
            sessions: Vec::new(),
            commands: Vec::new(),
            error: None,
            errors: Default::default(),
            sign_in: None,
            carried: Vec::new(),
            attachment_ingestion_busy: false,
            width: panel_width(cx),
            canvas_width: DEFAULT_CANVAS_WIDTH,
            canvas_focus: cx.focus_handle(),
            reveal: 0,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
            _sign_in: sign_in_subscriptions(cx),
            _poller: polling_task(cx),
            _shutdown: shutdown_hook(cx),
        }
    }

    #[cfg(test)]
    fn new_copilot(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_engine(ChatEngine::Copilot, window, cx)
    }

    /// The chat the panel shows. The panel always keeps at least one tab, so
    /// closing the last one opens an empty chat rather than leaving none.
    fn tab(&self) -> &Tab {
        let index = self.active.min(self.tabs.len().saturating_sub(1));
        &self.tabs[index]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        let index = self.active.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[index]
    }

    // qual:api — GPUI lifecycle callback.
    pub fn on_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal = self.reveal.wrapping_add(1);
        if self.is_disabled() {
            cx.notify();
            return;
        }
        if self.handle.is_none() {
            self.start(cx);
        }
        self.input.update(cx, |input, cx| input.focus(window, cx));
        self.sync_toolbar(cx);
        cx.notify();
    }

    /// Only stores an already loaded snapshot; no service requests or notifications.
    // qual:api — shell passes loaded dashboard snapshots, with no UI notification.
    pub fn set_context(&mut self, context: Option<ContextRef>, _cx: &mut Context<Self>) {
        if self.current != context {
            self.current = context;
        }
    }

    pub fn set_context_provider(&mut self, provider: ContextProvider) {
        self.providers.context = Some(provider);
    }

    /// The app's tools for ACP sessions. A running runtime keeps the ones
    /// it started with; Reconnect reads them again.
    pub fn set_chat_tools(&mut self, provider: ChatToolsProvider) {
        self.providers.tools = Some(provider);
    }

    pub fn indicator(&self) -> &'static str {
        activity_label(self.is_busy(), self.needs_attention())
    }

    /// Whether any chat tab is running a turn.
    pub fn is_busy(&self) -> bool {
        self.tabs.iter().any(Tab::is_busy)
    }

    /// Whether any chat tab waits for an approval or an answer.
    // qual:api — read by the app shell's status bar.
    pub fn needs_attention(&self) -> bool {
        self.tabs.iter().any(Tab::needs_attention)
    }

    /// Opens or shuts a foldable transcript row of the chat shown.
    fn toggle_expanded(&mut self, key: &str) {
        let expanded = &mut self.tab_mut().expanded;
        if !expanded.remove(key) {
            expanded.insert(key.to_owned());
        }
    }

    /// Push the panel's state into the toolbar and the input after every
    /// change, so both follow the tab that is shown.
    pub(crate) fn sync_toolbar(&mut self, cx: &mut Context<Self>) {
        let model = self.toolbar_model();
        let read_only = self.tab().read_only;
        self.toolbar
            .update(cx, |toolbar, cx| toolbar.set_model(model, cx));
        self.input
            .update(cx, |input, cx| input.set_disabled(read_only, cx));
    }

    /// Publishes the saved sessions, with the tabs that hold them, for the
    /// dashboards that list them (see [`shared`]).
    pub(crate) fn publish_sessions(&self, cx: &mut App) {
        let open: Vec<(String, bool)> = self
            .tabs
            .iter()
            .filter_map(|tab| Some((tab.session.clone()?, tab.is_busy())))
            .collect();
        shared::publish(cx, shared::with_tabs(&self.sessions, &open));
    }
}

fn configured_acp_agent(cx: &App) -> subspace_agent::AcpAgent {
    services(cx)
        .map(|services| services.config.chat.acp.agent)
        .unwrap_or_default()
}

fn configured_engine(cx: &App) -> ChatEngine {
    services(cx)
        .map(|services| services.config.chat.engine)
        .unwrap_or_default()
}

/// The saved panel width, or the default when none was saved.
fn panel_width(cx: &App) -> f32 {
    prefs::get(cx, WIDTH_KEY)
        .and_then(|width| width.parse().ok())
        .filter(|width: &f32| width.is_finite())
        .unwrap_or(DEFAULT_WIDTH)
}

/// Stops the runtime when the application quits.
fn shutdown_hook(cx: &mut Context<ChatPanel>) -> Subscription {
    cx.on_app_quit(|chat, _| {
        let _ = chat
            .handle
            .as_ref()
            .map(|handle| handle.send(Command::Shutdown));
        async {}
    })
}

fn control(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .xsmall()
        .accessibility_label(label.clone())
        .label(label)
}
