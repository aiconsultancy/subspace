//! Copilot Agents dashboard: the Copilot cloud agent tasks of the work account and
//! the local Copilot chat sessions, in one list.
//!
//! Cloud tasks come from `GET /agents/tasks` and from the saved task
//! references in `copilot-agent-tasks.json`, so a task the chat tool
//! started always has a row and carries its subject. Local chats come from
//! the chat panel, which publishes them (`subspace_framework::dashboard::chat_sessions`);
//! the dashboard asks the shell to open one with
//! [`subspace_framework::dashboard::ChatRequest`] and never touches the runtime.
//!
//! The dashboard owns the watcher. Dashboards stay alive while hidden, so
//! the watcher polls the monitored running tasks whichever dashboard shows,
//! and raises a toast, a sound and a notification when one of them finishes
//! or asks for an answer.

mod actions;
mod agent_tools;
mod browser;
mod dashboard;
mod dates;
mod filters;
mod filters_ui;
mod keys;
mod load;
mod logs;
mod merge;
mod model;
mod palette;
mod panels;
mod rows;
mod search_index;
mod ui;
mod view;
mod watch;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use gpui_kit::{App, Entity, EventEmitter, FocusHandle, Subscription, Task as GpuiTask};
use subspace_framework::client_github::Client as GithubClient;
use subspace_framework::client_github::copilot_agents::{Task, TaskDetail};
use subspace_framework::core::copilot_agent_tasks::{SubjectTaskRef, TaskStore};
use subspace_framework::core::keyed_request::KeyedRequest;
use subspace_framework::core::task_state::PollBackoff;
use subspace_framework::dashboard::list::{ListState, LoadStatus, Row};
use subspace_framework::dashboard::menu::Menu;
use subspace_framework::dashboard::overflow_toolbar::OverflowToolbar;
use subspace_framework::dashboard::{DashboardEvent, ItemSearch, SearchDoc, chat_sessions};

use crate::filters::{Counts, Filters};
use crate::keys::Picking;
use crate::model::AgentRow;

pub struct CopilotAgents {
    focus: FocusHandle,
    client: Option<Arc<GithubClient>>,
    store: Option<Arc<TaskStore>>,
    list: ListState,
    /// What `GET /agents/tasks` last returned.
    tasks: Vec<Task>,
    /// What `copilot-agent-tasks.json` last held.
    saved: Vec<SubjectTaskRef>,
    /// Tasks a detail load or a poll filled in since the list loaded.
    known: HashMap<String, Task>,
    /// Every merged row, before the filters.
    all_rows: Vec<AgentRow>,
    /// The rows the list shows.
    rows: Vec<AgentRow>,
    counts: Counts,
    filters: Filters,
    /// The open panel's task, and which request it came from, so a slow
    /// answer for a row the user already left is dropped.
    detail: Option<TaskDetail>,
    detail_key: Option<String>,
    detail_error: Option<String>,
    detail_request: KeyedRequest<String>,
    /// The one session log the panel shows, and the request that fills it.
    log: Option<logs::OpenLog>,
    log_request: u64,
    loading: bool,
    error: Option<String>,
    loaded_at: Option<Instant>,
    /// The state the watcher last saw, by task id.
    previous: HashMap<String, String>,
    backoff: PollBackoff,
    window_active: bool,
    menu: Option<(Entity<Menu>, Picking, Subscription)>,
    menu_anchors: subspace_framework::dashboard::menu::MenuAnchors,
    search: Rc<ItemSearch>,
    _watcher: GpuiTask<()>,
    _subscriptions: Vec<Subscription>,
    pub(crate) overflow_toolbar: Option<Entity<OverflowToolbar>>,
}

impl CopilotAgents {
    /// The row behind a list key.
    pub(crate) fn row(&self, key: &str) -> Option<&AgentRow> {
        self.all_rows.iter().find(|row| row.key == key)
    }

    /// What the row actions act on: the open panel's row, else the selected
    /// one. A group header stands for nothing here.
    pub(crate) fn target(&self) -> Option<&AgentRow> {
        let key = self
            .list
            .open_row()
            .map(|row| &row.key)
            .or(self.list.selected())?;
        self.row(key)
    }

    // ---- Rows --------------------------------------------------------------------------

    /// Merges the cloud tasks, the saved references and the chat sessions,
    /// sorts them, and gives the filtered result to the list.
    fn rebuild(&mut self, cx: &App) {
        let mut merged = merge::cloud_rows(&self.tasks, &self.saved, &self.known);
        merged.extend(merge::local_rows(&chat_sessions::sessions(cx)));
        merge::sort_rows(&mut merged);
        self.counts = filters::counts(&merged);
        self.all_rows = merged;
        self.rows = filters::filter_rows(self.all_rows.clone(), &self.filters);
        self.refresh_search();
        let status = self.load_status();
        self.list.set_groups(rows::groups(&self.rows, Utc::now()));
        self.list.set_status(status);
    }

    fn load_status(&self) -> LoadStatus {
        match &self.error {
            Some(message) if self.loaded_at.is_some() => LoadStatus::Failed {
                title: "Showing the last list. Refresh failed:".into(),
                message: message.clone().into(),
            },
            Some(message) => LoadStatus::Failed {
                title: "Cannot load the agent tasks".into(),
                message: message.clone().into(),
            },
            None if self.loaded_at.is_some() => LoadStatus::Ready,
            None => LoadStatus::Loading,
        }
    }

    fn refresh_search(&mut self) {
        let now = Utc::now();
        let items: Vec<(SearchDoc, Row)> = self
            .all_rows
            .iter()
            .map(|row| {
                let doc = SearchDoc::new(row.key.clone(), row.title.clone())
                    .parent(row.repo.clone())
                    .meta(format!(
                        "{} {} {} agent task session",
                        row.kind.id(),
                        row.state,
                        row.id
                    ));
                (doc, rows::list_row(row, now))
            })
            .collect();
        self.search = Rc::new(ItemSearch::new(items));
    }
}

impl EventEmitter<DashboardEvent> for CopilotAgents {}

impl CopilotAgents {
    /// The toolbar's summary line.
    fn status_text(&self) -> String {
        let filtered = match self.rows.len() == self.all_rows.len() {
            true => String::new(),
            false => format!(" · {} shown", self.rows.len()),
        };
        format!(
            "{} rows · {} active · {} waiting for input{filtered}",
            self.all_rows.len(),
            self.counts.active,
            self.counts.waiting
        )
    }
}
