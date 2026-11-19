//! Default-off GitHub Actions dashboard. All service work runs off the UI thread.

mod dashboard;
mod dialogs;
mod fetch;
mod filter_popup;
pub mod graph;
mod keys;
mod load;
mod menus;
mod model;
mod notes;
mod panel;
mod poll;
mod rows;
#[cfg(test)]
mod test_support;
mod toolbar;
mod view;
mod writes;

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use fetch::Fetch;
use gpui_kit::{Context, Entity, FocusHandle, Task};
use subspace_framework::client_github::Client;
use subspace_framework::core::task_state::PollBackoff;
use subspace_framework::dashboard::list::{GroupNote, ListState, LoadStatus};
use subspace_framework::dashboard::overflow_toolbar::OverflowToolbar;
use subspace_framework::dashboard::{ItemSearch, SearchDoc, prefs};

use model::{Filters, Record, Snapshot, View};

pub struct Actions {
    focus: FocusHandle,
    client: Option<Arc<Client>>,
    repos: Vec<String>,
    accounts: Vec<String>,
    login: Option<String>,
    generation: u64,
    data: HashMap<String, Snapshot>,
    list: ListState,
    /// The quiet line under each group with no rows, decided in `rebuild`.
    notes: HashMap<gpui_kit::SharedString, GroupNote>,
    search: Rc<ItemSearch>,
    view: View,
    filters: Filters,
    fetch: Fetch,
    visible: bool,
    error: Option<String>,
    loaded_at: Option<Instant>,
    last_full_load_started_at: Option<Instant>,
    queued_repos: BTreeSet<String>,
    poll: Option<Task<()>>,
    backoff: PollBackoff,
    toolbar: Option<Entity<OverflowToolbar>>,
    filter_popup: Option<Entity<filter_popup::FilterPopup>>,
    graph: Entity<graph::FlowGraph>,
    detail: panel::RunDetail,
    pending: Option<dialogs::Confirmation>,
    menu: Option<Entity<subspace_framework::dashboard::menu::Menu>>,
    menu_anchors: subspace_framework::dashboard::menu::MenuAnchors,
    /// Follows the favourite repositories in the running config.
    _config: gpui_kit::Subscription,
}

impl Actions {
    fn snapshot(&self) -> Option<&Snapshot> {
        self.data.get(&self.login.as_ref()?.to_lowercase())
    }

    fn records(&self) -> &[Record] {
        let Some(data) = self.snapshot() else {
            return &[];
        };
        match self.view {
            View::Runs | View::Flow => &data.runs,
            View::Deployments => &data.deployments,
            View::Releases => &data.releases,
        }
    }

    fn selected(&self) -> Option<&Record> {
        let key = self.list.selected()?;
        self.records()
            .iter()
            .find(|record| record.key == key.as_ref())
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let loading = self.fetch.loading_repos(&self.repos);
        let loaded = self
            .snapshot()
            .map(|data| data.loaded.clone())
            .unwrap_or_default();
        let progress = notes::Progress {
            loading: &loading,
            loaded: &loaded,
        };
        let groups = notes::visible_groups(
            rows::groups(self.records(), &self.repos, &self.filters, self.view),
            self.view,
            &self.filters,
            &progress,
            self.error.is_some(),
        );
        self.notes = notes::group_notes(&groups, self.view, &self.filters, &progress);
        let items = self
            .records()
            .iter()
            .map(|record| {
                (
                    SearchDoc::new(record.key.clone(), record.title.clone()),
                    rows::row(record),
                )
            })
            .collect::<Vec<_>>();
        self.list.set_groups(groups);
        self.list.set_status(self.load_status());
        self.search = Rc::new(ItemSearch::new(items));
        cx.notify();
    }

    fn load_status(&self) -> LoadStatus {
        if let Some(error) = &self.error {
            return LoadStatus::Failed {
                title: "Refresh failed; loaded records retained".into(),
                message: error.clone().into(),
            };
        }
        if !self.fetch.is_idle() && self.records().is_empty() {
            LoadStatus::Loading
        } else {
            LoadStatus::Ready
        }
    }

    fn close_filters(&mut self, cx: &mut Context<Self>) {
        self.filter_popup = None;
        cx.notify();
    }

    fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.filter_popup = None;
        self.view = view;
        prefs::set(cx, "actions.view", view.title());
        self.rebuild(cx);
    }

    fn status_text(&self) -> String {
        let runs = self.snapshot().map_or(&[][..], |data| data.runs.as_slice());
        let active = runs.iter().filter(|run| run.active).count();
        let age = notes::freshness(
            self.fetch.loads_records(),
            self.loaded_at.map(|at| at.elapsed().as_secs()),
        );
        format!(
            "{} runs · {active} active · {} · {age}",
            runs.len(),
            self.filters.status.title()
        )
    }

    fn switch_account(&mut self, login: String, cx: &mut Context<Self>) {
        if self.pending.is_some() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.fetch = Fetch::Idle;
        self.queued_repos.clear();
        self.detail = Default::default();
        self.login = Some(login.clone());
        prefs::set(cx, "actions.account", login);
        self.graph = panel::new_graph(cx);
        self.rebuild(cx);
        self.load_all(cx);
    }
}
