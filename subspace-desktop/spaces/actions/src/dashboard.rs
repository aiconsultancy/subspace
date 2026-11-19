//! Dashboard lifecycle and shell integration.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{App, Context, EventEmitter, FocusHandle, Focusable, SharedString, Window};
use subspace_framework::client_github::Client;
use subspace_framework::core::task_state::PollBackoff;
use subspace_framework::dashboard::list::{self, ListState, Row};
use subspace_framework::dashboard::{
    Dashboard, DashboardEvent, ItemSearch, Modal, PaletteCommand, Search, services,
};

use crate::{Actions, keys, model, panel};

impl EventEmitter<DashboardEvent> for Actions {}

impl Focusable for Actions {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Dashboard for Actions {
    const ID: &'static str = "actions";
    const TITLE: &'static str = "Actions";
    const SEARCHABLE: bool = false;
    const ICON: Option<&'static str> = Some("icons/workflow-16.svg");
    const ARTWORK: subspace_framework::dashboard::BoardArtwork =
        subspace_framework::dashboard::BoardArtwork::new(&[(
            "icons/workflow-16.svg",
            include_bytes!("../assets/workflow-16.svg"),
        )])
        .with_order(7);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");
    const HELP_IMAGES: &'static [subspace_framework::dashboard::HelpImage] =
        &[subspace_framework::dashboard::HelpImage::new(
            "flow",
            "Synthetic Flow graph with build, test, package and deployment jobs",
            "image/webp",
            include_bytes!("../help-assets/flow.webp"),
        )];

    fn settings() -> subspace_framework::dashboard::settings::BoardSettings {
        subspace_framework::dashboard::settings::BoardSettings {
            description: "Shows GitHub Actions workflows, runs, deployments and releases for configured repositories. You can inspect jobs and trigger, rerun or cancel workflows. This experimental board is disabled by default, and changes require confirmation.",
            background_updates: true,
            ..Default::default()
        }
    }

    fn connection_settings() -> subspace_framework::dashboard::settings::ConnectionSettings {
        use subspace_framework::dashboard::settings::{ConnectionField, ConnectionSettings};
        ConnectionSettings::new(&[
            ConnectionField::GitHubCredentials,
            ConnectionField::Repositories,
        ])
    }

    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved_view = subspace_framework::dashboard::prefs::get(cx, "actions.view");
        let view = model::View::ALL
            .into_iter()
            .find(|view| Some(view.title()) == saved_view.as_deref())
            .unwrap_or_default();
        let filters = model::Filters {
            branch: subspace_framework::dashboard::prefs::get(cx, "actions.branch")
                .filter(|value| !value.is_empty()),
            workflow: subspace_framework::dashboard::prefs::get(cx, "actions.workflow")
                .filter(|value| !value.is_empty()),
            actor: subspace_framework::dashboard::prefs::get(cx, "actions.actor")
                .filter(|value| !value.is_empty()),
            status: model::StatusFilter::ALL
                .into_iter()
                .find(|status| {
                    Some(status.title())
                        == subspace_framework::dashboard::prefs::get(cx, "actions.status")
                            .as_deref()
                })
                .unwrap_or_default(),
            by_status: subspace_framework::dashboard::prefs::get(cx, "actions.grouping").as_deref()
                == Some("Status"),
        };
        let client = services(cx).map(|s| Arc::new(Client::new(s.credentials.clone())));
        let repos = services(cx)
            .map(|s| s.config.repos.clone())
            .unwrap_or_default();
        Self {
            focus: cx.focus_handle(),
            client,
            repos,
            accounts: Vec::new(),
            login: None,
            generation: 0,
            data: HashMap::new(),
            list: ListState::default(),
            notes: HashMap::new(),
            search: Rc::new(ItemSearch::new([])),
            view,
            filters,
            fetch: Default::default(),
            visible: false,
            error: None,
            loaded_at: None,
            last_full_load_started_at: None,
            queued_repos: Default::default(),
            poll: None,
            backoff: PollBackoff::default(),
            toolbar: None,
            filter_popup: None,
            graph: panel::new_graph(cx),
            detail: panel::RunDetail::default(),
            pending: None,
            menu: None,
            menu_anchors: Default::default(),
            _config: cx
                .observe_global::<subspace_framework::dashboard::Services>(Self::follow_repos),
        }
    }

    fn bind_keys(cx: &mut App) {
        keys::bind(cx);
    }

    fn activate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        if self.loaded_at.is_none() {
            self.refresh_all(cx);
        }
        crate::poll::start_poll(self, cx);
    }

    fn background_refresh(&mut self, cx: &mut Context<Self>) {
        if subspace_framework::dashboard::refresh::enabled(Self::ID, cx) && self.full_load_due(cx) {
            self.refresh_all(cx);
        }
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
        self.filter_popup = None;
        self.visible = false;
        self.poll = None;
    }

    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_all(cx);
    }

    fn busy(&self, _cx: &App) -> bool {
        !self.fetch.is_idle()
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        Some(self.status_text().into())
    }

    fn badge(&self, _cx: &App) -> Option<SharedString> {
        let count = self.snapshot().map_or(0, |data| {
            data.deployments
                .iter()
                .filter(|r| r.approval.is_some())
                .count()
        });
        (count > 0).then(|| count.to_string().into())
    }

    fn subject(&self, _cx: &App) -> Option<Row> {
        self.list.selected_row().cloned()
    }

    fn commands(&self, cx: &Context<Self>) -> Vec<PaletteCommand> {
        let mut commands = vec![PaletteCommand::new("Filter Actions…", keys::Status).shortcut("s")];
        if let Some(repo) = panel::selected_repository(self).filter(|_| self.login.is_some()) {
            let repo = repo.to_owned();
            let board = cx.weak_entity();
            commands.push(PaletteCommand::callback(
                "Refresh repository",
                move |_, cx| {
                    _ = board.update(cx, |this, cx| this.refresh_repo(repo.clone(), cx));
                },
            ));
        }
        commands
    }

    fn search(&self, _cx: &App) -> Option<Search> {
        Some(Search {
            placeholder: "Search loaded Actions records".into(),
            items: self.search.clone(),
        })
    }

    fn open_search_result(&mut self, id: &str, _window: &mut Window, cx: &mut Context<Self>) {
        list::update(self, cx, |list| {
            list.reveal(id);
        });
    }

    fn modal(&self, _cx: &App) -> Option<Modal> {
        if self.pending.is_some() {
            Some(Modal::Dialog)
        } else if self.filter_popup.is_some() {
            Some(Modal::Menu)
        } else {
            self.menu.as_ref().map(|_| Modal::Menu)
        }
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
        self.filter_popup = None;
        if !self.pending.as_ref().is_some_and(|dialog| dialog.busy) {
            self.pending = None;
        }
        cx.notify();
    }

    fn keys(&self) -> &'static [subspace_framework::dashboard::KeyHelp] {
        keys::HELP
    }
}
