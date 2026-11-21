use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{App, Context, KeyBinding, SharedString, Window};
use subspace_framework::client_github::Client as GithubClient;
use subspace_framework::core::copilot_agent_tasks::TaskStore;
use subspace_framework::core::credentials::GITHUB_ACCOUNT_KEY;
use subspace_framework::core::task_state::PollBackoff;
use subspace_framework::dashboard::list::{self, ListState, Row};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{
    Dashboard, DashboardEvent, ItemSearch, KeyHelp, Modal, PaletteCommand, Search, chat_sessions,
    prefs, services, view_keys,
};

use crate::filters::{Counts, Filters};
use crate::keys::{
    ClearFilters, CopyLink, GROUP_KEY, GroupMenu, HELP_FOOT, KEYS, KIND_KEY, KindMenu, OpenInChat,
    OpenPullRequest, OpenSession, OpenSubject, REPO_KEY, RepoMenu, SEARCH_PLACEHOLDER, ShowLog,
    ToggleMonitor,
};
use crate::model::{Kind, StateGroup};
use crate::{CopilotAgents, agent_tools, filters, load, search_index, shell};

impl Dashboard for CopilotAgents {
    const ID: &'static str = "github-copilot-agents";
    const TITLE: &'static str = "Copilot Agents";
    const SEARCHABLE: bool = true;
    const ICON: Option<&'static str> = Some("logos/copilot-agents.svg");
    const ARTWORK: subspace_framework::dashboard::BoardArtwork =
        subspace_framework::dashboard::BoardArtwork::new(&[(
            "logos/copilot-agents.svg",
            include_bytes!("../assets/logos/copilot-agents.svg"),
        )])
        .with_order(1);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn settings() -> subspace_framework::dashboard::settings::BoardSettings {
        subspace_framework::dashboard::settings::BoardSettings {
            description: "Shows recent GitHub Copilot cloud agent tasks and saved local chat sessions. You can filter tasks, inspect session logs and monitor cloud tasks for alerts. The board does not start, steer or stop tasks.",
            background_updates: true,
            ..Default::default()
        }
    }

    fn connection_settings() -> subspace_framework::dashboard::settings::ConnectionSettings {
        use subspace_framework::dashboard::settings::{ConnectionField, ConnectionSettings};
        ConnectionSettings::new(&[
            ConnectionField::GitHubCredentials,
            ConnectionField::Repositories,
            ConnectionField::Note(
                "Configure task filters, alerts and agent behavior in the Copilot Agents dashboard.",
            ),
        ])
    }

    fn agent_tools(
        environment: &subspace_framework::dashboard::ToolEnvironment,
    ) -> Vec<Arc<dyn subspace_framework::dashboard::AgentTool>> {
        agent_tools::agent_tools(environment)
    }

    fn open_indexed_result_fenced(
        &mut self,
        hit: &subspace_framework::dashboard::full_text::Hit,
        permit: subspace_framework::dashboard::full_text::ReadPermit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        search_index::open(self, hit, permit, window, cx);
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = services(cx).map(|services| {
            Arc::new(
                GithubClient::new(services.credentials.clone())
                    .with_work_account(prefs::get(cx, GITHUB_ACCOUNT_KEY)),
            )
        });
        let filters = Filters {
            kind: prefs::get(cx, KIND_KEY).and_then(|id| Kind::from_id(&id)),
            group: prefs::get(cx, GROUP_KEY).and_then(|id| StateGroup::from_id(&id)),
            repo: prefs::get(cx, REPO_KEY).filter(|repo| !repo.is_empty()),
        };
        let (navigator_open, inspector_open) = shell::stored_docks(cx);
        Self {
            focus: cx.focus_handle(),
            client,
            store: TaskStore::from_system().map(Arc::new),
            list: ListState::default(),
            tasks: Vec::new(),
            saved: Vec::new(),
            known: HashMap::new(),
            all_rows: Vec::new(),
            rows: Vec::new(),
            counts: Counts::default(),
            filters,
            detail: None,
            detail_key: None,
            detail_error: None,
            detail_request: Default::default(),
            log: None,
            log_request: 0,
            loading: false,
            error: None,
            loaded_at: None,
            login: None,
            navigator_open,
            inspector_open,
            item: Default::default(),
            previous: HashMap::new(),
            backoff: PollBackoff::new(),
            window_active: window.is_window_active(),
            menu: None,
            menu_anchors: Default::default(),
            search: Rc::new(ItemSearch::new([])),
            _watcher: load::watcher(cx),
            _subscriptions: vec![
                cx.observe_window_activation(window, |this, window, _| {
                    this.window_active = window.is_window_active();
                }),
                cx.observe_global::<chat_sessions::ChatSessions>(|this, cx| {
                    this.rebuild(cx);
                    cx.notify();
                }),
            ],
            overflow_toolbar: None,
        }
    }

    fn bind_keys(cx: &mut App) {
        let context = view_keys(Self::ID);
        let context = Some(context.as_str());
        cx.bind_keys([
            KeyBinding::new("1", KindMenu, context),
            KeyBinding::new("2", GroupMenu, context),
            KeyBinding::new("3", RepoMenu, context),
            KeyBinding::new("0", ClearFilters, context),
            KeyBinding::new("o", OpenSession, context),
            KeyBinding::new("p", OpenPullRequest, context),
            KeyBinding::new("y", CopyLink, context),
            KeyBinding::new("m", ToggleMonitor, context),
            KeyBinding::new("u", OpenSubject, context),
            KeyBinding::new("c", OpenInChat, context),
            KeyBinding::new("l", ShowLog, context),
        ]);
    }

    fn activate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DashboardEvent::Chat(
            subspace_framework::dashboard::ChatRequest::ListSessions,
        ));
        self.load_all(cx);
    }

    fn background_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.loading && subspace_framework::dashboard::refresh::enabled(Self::ID, cx) {
            self.load_all(cx);
        }
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
        cx.notify();
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        Some(self.status_text().into())
    }

    fn docks(&self, _cx: &App) -> SpaceDocks {
        self.space_docks()
    }

    fn toggle_dock(&mut self, dock: SpaceDock, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_space_dock(dock, cx);
    }

    fn badge(&self, _cx: &App) -> Option<SharedString> {
        filters::badge(self.counts).map(SharedString::from)
    }

    fn busy(&self, _cx: &App) -> bool {
        self.loading
    }

    /// The opened row while its tab shows, else the selected row.
    fn subject(&self, _cx: &App) -> Option<Row> {
        self.target()
            .map(|row| crate::rows::list_row(row, chrono::Utc::now()))
    }

    fn commands(&self, cx: &Context<Self>) -> Vec<PaletteCommand> {
        self.palette_commands(cx)
    }

    fn search(&self, _cx: &App) -> Option<Search> {
        Some(Search {
            placeholder: SEARCH_PLACEHOLDER.into(),
            items: self.search.clone(),
        })
    }

    fn open_search_result(&mut self, id: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let id = id.to_owned();
        list::update(self, cx, |list| {
            list.reveal(&id);
        });
    }

    fn modal(&self, _cx: &App) -> Option<Modal> {
        self.menu.as_ref().map(|_| Modal::Menu)
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
        cx.notify();
    }

    fn keys(&self) -> &'static [KeyHelp] {
        KEYS
    }

    fn help_foot(&self) -> Option<SharedString> {
        Some(HELP_FOOT.into())
    }

    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.loaded_at = None;
        self.known.clear();
        cx.emit(DashboardEvent::Chat(
            subspace_framework::dashboard::ChatRequest::ListSessions,
        ));
        self.load_all(cx);
    }
}
