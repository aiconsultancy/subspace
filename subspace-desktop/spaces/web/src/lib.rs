//! Web boards: each `webBoards` entry in the config file is a board that
//! shows one web page, or a web app from a folder of built files, in a
//! native web view. The app registers one instance per entry with
//! [`create`].

mod app;
mod bridge;
pub mod install;
pub mod manager;
mod manifest;
mod page;
mod policy;
mod source;
mod state;
mod toolbar;

use std::time::{Duration, Instant};

use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window, div,
};
use subspace_framework::core::config::web_boards::{WebBoard as WebBoardConfig, WebBoardSource};
use subspace_framework::dashboard::{
    Dashboard, DashboardEvent, DashboardHandle, KeyHelp, Tokens, feedback,
};

use page::{Page, Profile, Start};
pub use source::{description, settings_fields};
use state::PageState;
use toolbar::Navigation;

/// How often the board reads its page's events.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const INSTANCE: &str = "web-board";

/// The view of the configured web board `board`.
pub fn create(board: &WebBoardConfig, _window: &mut Window, cx: &mut App) -> DashboardHandle {
    let config = board.clone();
    DashboardHandle::from_entity(cx.new(|cx| WebBoard::with_config(config, cx)))
}

pub struct WebBoard {
    config: WebBoardConfig,
    start: Start,
    focus: FocusHandle,
    /// Created the first time the board is shown.
    page: Option<Page>,
    /// Why the page could not be created.
    error: Option<SharedString>,
    state: PageState,
    active: bool,
    covered: bool,
    last_external: Option<Instant>,
    bridge: Option<bridge::Bridge>,
}

impl WebBoard {
    fn with_config(config: WebBoardConfig, cx: &mut Context<Self>) -> Self {
        Self::watch(cx);
        Self {
            start: source::start(&config),
            bridge: bridge::Bridge::new(&config),
            config,
            focus: cx.focus_handle(),
            page: None,
            error: None,
            state: PageState::default(),
            active: false,
            covered: false,
            last_external: None,
        }
    }

    fn ensure_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page.is_some() || self.error.is_some() {
            return;
        }
        let opened = self.open_page(window, cx);
        self.attach(opened);
        cx.notify();
    }

    fn attach(&mut self, opened: Result<Page, String>) {
        match opened {
            Ok(page) => self.page = Some(page),
            Err(error) => {
                subspace_framework::core::log::Logger::new(INSTANCE).record(
                    subspace_framework::core::log::Level::Error,
                    format!("{}: {error}", self.config.id),
                    &[],
                );
                self.error = Some(error.into());
            }
        }
    }

    fn open_page(&self, window: &mut Window, cx: &mut Context<Self>) -> Result<Page, String> {
        if let Some(problem) = source::problem(&self.config) {
            return Err(problem);
        }
        let profile = Profile::for_board(&self.config.id, cx);
        Page::open(&self.start, &profile, window, cx)
    }

    fn watch(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                if this.update(cx, |board, cx| board.poll(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let Some(page) = &self.page else {
            return;
        };
        let (events, dropped) = page.take_events();
        if events.is_empty() && !dropped {
            return;
        }
        let current = dropped.then(|| page.url(cx)).flatten();
        if dropped {
            self.state.resync(current);
        }
        for event in events {
            self.page_event(event, cx);
        }
        cx.notify();
    }

    fn page_event(&mut self, event: page::PageEvent, cx: &mut Context<Self>) {
        if let page::PageEvent::Message {
            source,
            body,
            epoch,
        } = event
        {
            if let (Some(page), Some(bridge), Some(config)) = (
                &self.page,
                &mut self.bridge,
                bridge::runtime::live_config(&self.config.id, cx).cloned(),
            ) {
                bridge::runtime::receive(
                    bridge,
                    page,
                    &config,
                    bridge::runtime::Incoming {
                        source: &source,
                        body: &body,
                        epoch,
                    },
                    cx,
                );
            }
            return;
        }
        if matches!(event, page::PageEvent::Loading(_))
            && let Some(bridge) = &mut self.bridge
        {
            bridge.badge = None;
            bridge.context = None;
        }
        if let Some(url) = self.state.apply(event) {
            self.open_externally(&url, cx)
        }
    }

    fn open_externally(&mut self, url: &str, cx: &mut Context<Self>) {
        let now = Instant::now();
        if policy::may_open_externally(self.last_external, now) {
            self.last_external = Some(now);
            cx.open_url(url);
        }
    }

    /// Show the native view only while the board is visible and nothing
    /// covers it.
    fn sync_shown(&self, cx: &mut Context<Self>) {
        if let Some(page) = &self.page {
            page.set_shown(self.active && !self.covered, cx);
        }
    }

    pub(crate) fn navigate(&mut self, navigation: Navigation, cx: &mut Context<Self>) {
        let Some(page) = &self.page else {
            return;
        };
        let result = toolbar::navigate(page, &self.start.url, navigation, cx);
        if let Err(error) = result {
            self.state.notice = Some(error);
            cx.notify();
        }
    }

    fn retry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        self.ensure_page(window, cx);
        self.sync_shown(cx);
    }

    fn body(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = Tokens::current(cx);
        if let Some(page) = &self.page {
            return page.view().into_any_element();
        }
        match &self.error {
            Some(error) => feedback::error_box(
                &tokens,
                INSTANCE,
                "Cannot show this page",
                error.clone(),
                cx.listener(|board, _: &ClickEvent, window, cx| board.retry(window, cx)),
            )
            .into_any_element(),
            None => feedback::loading_line(&tokens, "web-board-opening", "Opening the page…")
                .p(tokens.pad_x)
                .into_any_element(),
        }
    }
}

impl EventEmitter<DashboardEvent> for WebBoard {}

impl Focusable for WebBoard {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WebBoard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(SharedString::from(format!("{INSTANCE}-{}", self.config.id)))
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .child(self.body(cx))
    }
}

impl Dashboard for WebBoard {
    const ID: &'static str = "web";
    const TITLE: &'static str = "Web";
    const DESCRIPTION: &'static str = "A web page from the config file";
    const ICON: Option<&'static str> = Some("icons/globe.svg");
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn settings() -> subspace_framework::dashboard::settings::BoardSettings {
        subspace_framework::dashboard::settings::BoardSettings {
            description: "Shows a website or a built local web app in a native web view. You can navigate the page and manage supported app settings and permissions. Web Spaces are available on macOS and Windows, not Linux.",
            notifications: true,
            ..Default::default()
        }
    }

    fn badge(&self, cx: &App) -> Option<SharedString> {
        bridge::runtime::badge(
            self.bridge.as_ref()?,
            self.page.as_ref()?,
            bridge::runtime::live_config(&self.config.id, cx)?,
            cx,
        )
    }

    fn agent_context(&self, cx: &App) -> Option<subspace_framework::dashboard::AgentContext> {
        bridge::runtime::context(
            self.bridge.as_ref()?,
            self.page.as_ref()?,
            bridge::runtime::live_config(&self.config.id, cx)?,
            cx,
        )
    }

    /// Only for the dashboard interface: the app creates web boards with
    /// [`create`], from the config file.
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = WebBoardConfig {
            id: Self::ID.to_owned(),
            title: Self::TITLE.to_owned(),
            source: WebBoardSource::Url(String::new()),
            settings: Default::default(),
            secrets: Default::default(),
            permissions: Default::default(),
        };
        Self::with_config(config, cx)
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = true;
        self.ensure_page(window, cx);
        self.sync_shown(cx);
    }

    fn deactivate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.active = false;
        self.sync_shown(cx);
    }

    fn covered(&mut self, covered: bool, _window: &mut Window, cx: &mut Context<Self>) {
        self.covered = covered;
        self.sync_shown(cx);
    }

    fn window_title(&self, _cx: &App) -> Option<SharedString> {
        self.state.title.clone().map(SharedString::from)
    }

    fn render_toolbar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        Some(toolbar::render(
            self.page.is_some(),
            self.start.app.is_none(),
            cx,
        ))
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        self.state.status().map(SharedString::from)
    }

    fn busy(&self, _cx: &App) -> bool {
        self.state.loading
    }

    fn keys(&self) -> &'static [KeyHelp] {
        &[]
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page.is_some() {
            self.navigate(Navigation::Reload, cx);
        } else {
            self.retry(window, cx);
        }
    }
}
