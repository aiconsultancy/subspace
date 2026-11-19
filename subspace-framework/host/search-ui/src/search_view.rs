//! Native, offline-first full-text search surface shared by Subspace products.

mod monitor;
mod operations;
mod policy;
mod view;

#[cfg(test)]
mod lifecycle_tests;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle,
    Subscription, Window,
};

use crate::full_text::{Hit, Policy, RerankerStatus, SearchResponse, Service, SourceStatus};

pub enum SearchViewEvent {
    Close,
    Open(Box<Hit>),
}

struct SourceEditor {
    board: String,
    policy: Policy,
    days: Entity<InputState>,
    gigabytes: Entity<InputState>,
}

pub struct SearchView {
    service: Service,
    boards: Vec<(String, String)>,
    current: String,
    input: Entity<InputState>,
    all: bool,
    rerank: bool,
    response: Option<SearchResponse>,
    selected: Option<usize>,
    scroll: ScrollHandle,
    sources: Vec<SourceStatus>,
    editors: Vec<SourceEditor>,
    model: Option<RerankerStatus>,
    generation: u64,
    source_generation: u64,
    loading: bool,
    managing: bool,
    busy: bool,
    installing: bool,
    confirm_clear: Option<String>,
    error: Option<String>,
    notice: Option<String>,
    _subscription: Subscription,
}

impl EventEmitter<SearchViewEvent> for SearchView {}

impl Focusable for SearchView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl SearchView {
    pub fn new(
        service: Service,
        boards: Vec<(String, String)>,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        view::bind_keys(cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search cached content…"));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event, window, cx| match event {
                InputEvent::Change => {
                    this.invalidate(cx);
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        );
        let mut view = Self {
            service,
            boards,
            current,
            input,
            all: false,
            rerank: false,
            response: None,
            selected: None,
            scroll: ScrollHandle::new(),
            sources: Vec::new(),
            editors: Vec::new(),
            model: None,
            generation: 0,
            source_generation: 0,
            loading: false,
            managing: false,
            busy: false,
            installing: false,
            confirm_clear: None,
            error: None,
            notice: None,
            _subscription: subscription,
        };
        view.refresh_sources(window, cx);
        view.monitor_sources(window, cx);
        view.focus(window, cx);
        view
    }

    // qual:api
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Call on board or account changes, including changes within the same board.
    // qual:api
    pub fn set_current_board(&mut self, board: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.current = board.to_owned();
        self.refresh_sources(window, cx);
    }

    fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.response = None;
        self.selected = None;
        self.loading = false;
        self.error = None;
        cx.notify();
    }

    fn board_name(&self, board: &str) -> String {
        self.boards
            .iter()
            .find(|(id, _)| id == board)
            .map_or_else(|| board.to_owned(), |(_, name)| name.clone())
    }
}
