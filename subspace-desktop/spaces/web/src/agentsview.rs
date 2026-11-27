//! AgentsView opens the fleet's own web application in a scoped native view.

mod connection;
mod settings;

use std::time::{Duration, Instant};

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
};
use subspace_framework::dashboard::{
    Dashboard, DashboardEvent, Services, ThemeId, Tokens, feedback,
    settings::{BoardSettings, SettingsEditorFactory},
    workspace,
};

use crate::page::{Page, PageEvent};
pub(crate) use connection::Connection;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionEvent {
    Ready,
    Rejected,
    Unavailable,
}

impl ConnectionEvent {
    pub(crate) fn parse(message: &str) -> Option<Self> {
        match message {
            "ready" => Some(Self::Ready),
            "rejected" => Some(Self::Rejected),
            "unavailable" => Some(Self::Unavailable),
            _ => None,
        }
    }
}

enum State {
    Setup,
    Opening { page: Page, started: Instant },
    Connected(Page),
    KeyRequired,
    Failed(&'static str),
}

impl State {
    fn page(&self) -> Option<&Page> {
        match self {
            Self::Opening { page, .. } | Self::Connected(page) => Some(page),
            _ => None,
        }
    }
}

pub struct AgentsView {
    focus: FocusHandle,
    connection: Result<Option<Connection>, &'static str>,
    state: State,
    active: bool,
    covered: bool,
    _settings: Subscription,
    _theme: Subscription,
    _poll: gpui_kit::Task<()>,
    mirror: crate::mirror::Mirror,
}

impl AgentsView {
    fn connection(cx: &App) -> Result<Option<Connection>, &'static str> {
        match cx
            .try_global::<Services>()
            .and_then(|services| services.config.board_settings.get(Self::ID))
        {
            Some(saved) => Connection::from_saved(saved),
            None => Ok(None),
        }
    }

    fn update_connection(&mut self, connection: Result<Option<Connection>, &'static str>) -> bool {
        if self.connection == connection {
            return false;
        }
        self.connection = connection;
        self.state = State::Setup;
        true
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state = match &self.connection {
            Ok(Some(connection)) => match Page::open_agentsview(connection, window, cx) {
                Ok(page) => State::Opening {
                    page,
                    started: Instant::now(),
                },
                Err(error) => State::Failed(error),
            },
            Ok(None) => State::Setup,
            Err(error) => State::Failed(error),
        };
        self.mirror = crate::mirror::Mirror::new();
        cx.notify();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.state.page() else {
            return;
        };
        for event in page.take_events().0 {
            if let PageEvent::AgentsView(event) = event {
                self.connection_event(event);
                self.sync_shown(cx);
                cx.notify();
            }
        }
        if matches!(&self.state, State::Opening { started, .. } if started.elapsed() >= CONNECT_TIMEOUT)
        {
            self.state = State::Failed(
                "The server did not answer. Check the address and your connection to the fleet.",
            );
            cx.notify();
        }
        if self.active
            && !self.covered
            && let State::Connected(page) = &self.state
        {
            self.mirror
                .snapshot(page, |board: &mut Self| &mut board.mirror, cx);
        }
    }

    fn connection_event(&mut self, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Ready if matches!(self.state, State::Opening { .. }) => {
                if let State::Opening { page, .. } =
                    std::mem::replace(&mut self.state, State::Setup)
                {
                    self.state = State::Connected(page);
                }
            }
            ConnectionEvent::Rejected if self.state.page().is_some() => {
                self.state = State::KeyRequired;
            }
            ConnectionEvent::Unavailable if matches!(self.state, State::Opening { .. }) => {
                self.state = State::Failed(
                    "The server did not return AgentsView settings. Check the address and try again.",
                );
            }
            _ => {}
        }
    }

    fn sync_shown(&self, cx: &mut Context<Self>) {
        if let State::Connected(page) = &self.state {
            page.set_shown(self.active && !self.covered, cx);
        }
    }
}

impl Focusable for AgentsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AgentsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let body = match &self.state {
            State::Setup => feedback::configuration(
                &tokens,
                Self::ID,
                "Connect to your fleet",
                "Enter the AgentsView address and API key in Settings.",
            )
            .into_any_element(),
            State::KeyRequired => feedback::configuration(
                &tokens,
                Self::ID,
                "Set an AgentsView API key",
                "Your fleet requires a valid API key. Set it in Settings to open AgentsView.",
            )
            .into_any_element(),
            State::Opening { .. } => feedback::skeleton(&tokens).into_any_element(),
            State::Connected(page) => self
                .mirror
                .image_over(page.view().into_element().into_any_element()),
            State::Failed(error) => feedback::error_box_with_settings(
                &tokens,
                Self::ID,
                "Cannot open AgentsView",
                *error,
                cx.listener(|board, _, window, cx| board.open(window, cx)),
            )
            .into_any_element(),
        };
        div()
            .id(Self::ID)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .child(workspace::Pane::new("agentsview-pane").child(body))
    }
}

impl EventEmitter<DashboardEvent> for AgentsView {}

impl Dashboard for AgentsView {
    const ID: &'static str = "agentsview";
    const TITLE: &'static str = "AgentsView";
    const DESCRIPTION: &'static str = "Browse your fleet’s agent sessions in AgentsView";
    const ICON: Option<&'static str> = Some("icons/bot.svg");
    const HELP_MARKDOWN: &'static str = include_str!("../agentsview-help.md");

    fn settings() -> BoardSettings {
        BoardSettings {
            description: Self::DESCRIPTION,
            ..Default::default()
        }
    }

    fn settings_editor() -> Option<SettingsEditorFactory> {
        Some(settings::editor)
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.observe_global_in::<Services>(window, |board, window, cx| {
            if board.update_connection(Self::connection(cx)) {
                if board.active && !board.covered {
                    board.open(window, cx);
                }
                cx.notify();
            }
        });
        let theme = cx.observe_global::<ThemeId>(|board, cx| {
            if let (Ok(Some(connection)), Some(page)) = (&board.connection, board.state.page()) {
                page.update_agentsview_appearance(connection, cx);
            }
        });
        let poll = cx.spawn(async |this, cx| {
            loop {
                cx.background_executor().timer(crate::POLL_INTERVAL).await;
                if this.update(cx, Self::poll).is_err() {
                    break;
                }
            }
        });
        Self {
            focus: cx.focus_handle(),
            connection: Self::connection(cx),
            state: State::Setup,
            active: false,
            covered: false,
            _settings: settings,
            _theme: theme,
            _poll: poll,
            mirror: crate::mirror::Mirror::new(),
        }
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = true;
        if matches!(self.state, State::Setup) {
            self.open(window, cx);
        }
        self.sync_shown(cx);
    }

    fn deactivate(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.active = false;
        self.sync_shown(cx);
    }

    fn covered(&mut self, covered: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.covered = covered;
        if self.active
            && !covered
            && matches!(self.state, State::Setup)
            && self.connection != Ok(None)
        {
            self.open(window, cx);
        }
        self.sync_shown(cx);
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(window, cx);
    }

    fn status(&self, _: &App) -> Option<SharedString> {
        Some(
            match self.state {
                State::Setup => "AgentsView needs setup",
                State::Opening { .. } => "Connecting to AgentsView…",
                State::Connected(_) => "Connected to AgentsView",
                State::KeyRequired => "AgentsView API key required",
                State::Failed(_) => "AgentsView unavailable",
            }
            .into(),
        )
    }

    fn busy(&self, _: &App) -> bool {
        matches!(self.state, State::Opening { .. })
    }
}
