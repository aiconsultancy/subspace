//! GitHub dashboard: pull requests and issues (REQ-60 to REQ-63). Accounts from `gh`, six saved
//! searches, lists grouped by repository and shown from the cache first, the
//! detail panel, reviews, comments and assignees, and the diff view.

mod agent_context;
mod agent_tools;
mod close_pull_request;
mod detail;
mod diff;
mod favourites;
mod inspector;
mod layout;
mod merge_pull_request;
mod model;
mod navigation;
mod page;
mod peek;
mod references;
mod repo_filter;
mod rows;
#[cfg(test)]
mod sample;
mod search_index;
mod store;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Action, AnyElement, App, AppContext as _, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, KeyBinding, KeyContext, ParentElement as _,
    Pixels, Render, SharedString, Stateful, Styled as _, Subscription, Task, TestSupportExt as _,
    WeakEntity, Window, actions, div,
};
use subspace_framework::client_github::model::{
    GhItem, GhItemDetail, GhUser, ItemKind, PrState, ReviewEvent, SECTIONS, SectionDef, short_login,
};
use subspace_framework::client_github::{Client, lifecycle::REFRESH_INTERVAL};
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::composer::{Composer, ComposerEvent};
use subspace_framework::dashboard::list::{
    self, Detail, Group, GroupIndicator, ListDashboard, ListState, LoadStatus, Row,
};
use subspace_framework::dashboard::menu::{Menu, MenuAnchors, MenuContents, MenuEvent, MenuOption};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::row_menu::{MenuCommand, MenuGroup, RowMenu};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{
    DASHBOARD_CONTEXT, Dashboard, DashboardEvent, ItemSearch, KeyHelp, Modal, PaletteCommand,
    PaletteOption, Search, SubList, Toast, Tokens, links, panel, prefs, services, view_keys,
};

use crate::detail::{Mode, OnAction, PanelAction, PanelInput};
use crate::diff::{DiffEvent, DiffView, OpenDiff};
use crate::model::{
    SectionData, assignee_logins, can_comment, can_review, default_section, is_pr_section,
    patch_sections, pr_only, same_login, search_doc, search_items, section_by_id, section_repos,
    sections_of,
};

actions!(
    github,
    [
        /// Section 1: pull requests you opened.
        Section1,
        /// Section 2: review requested.
        Section2,
        /// Section 3: involved.
        Section3,
        /// Section 4: all open pull requests in the configured repositories.
        Section4,
        /// Switch account.
        SwitchAccount,
        /// Approve the pull request.
        Approve,
        /// Request changes.
        RequestChanges,
        /// Comment.
        Comment,
        /// Close the pull request after confirmation.
        ClosePullRequest,
        /// Change assignees.
        Assign,
        /// Assign to me, or unassign me.
        AssignMe,
        /// Files: the pull request's diff.
        ShowDiff,
        /// Open in GitHub.
        OpenInGithub,
        /// Copy the URL.
        CopyUrl,
        /// Repositories: search and star favourites.
        OpenRepositories,
        /// Star or unstar the selected repository.
        ToggleFavourite,
        /// Favourites only in Mine, Review requested and Involved, or not.
        ToggleFavouritesOnly,
        /// Show every repository again, not only the filtered one.
        ClearRepositoryFilter,
        /// Collapse the repository, or select the item's repository.
        CollapseGroup,
        /// Expand the repository.
        ExpandGroup,
    ]
);

/// The web app's `KEYS`.
const KEYS: &[KeyHelp] = &[
    KeyHelp::new(
        "j / k",
        "Next / previous row (on a page: next / previous pull request)",
    ),
    KeyHelp::new(
        "Enter",
        "Open the pull request's page; on a repository, collapse or expand it",
    ),
    KeyHelp::new("← / →", "Collapse / expand the repository"),
    KeyHelp::new(
        "1 – 4",
        "Sections: Mine, Review requested, Involved, All open",
    ),
    KeyHelp::new("d", "Files: the pull request's diff"),
    KeyHelp::new("u", "Switch account"),
    KeyHelp::new("F", "Repositories: search and star favourites"),
    KeyHelp::new("f", "Star or unstar the selected repository"),
    KeyHelp::new("A", "Approve the pull request"),
    KeyHelp::new("R", "Request changes"),
    KeyHelp::new("c", "Comment"),
    KeyHelp::new("x", "Close the pull request…"),
    KeyHelp::new("a", "Change assignees"),
    KeyHelp::new("i", "Assign to me, or unassign me"),
    KeyHelp::new("o", "Open in GitHub"),
    KeyHelp::new("y", "Copy the URL"),
    KeyHelp::new("Esc", "Back to the list from a page"),
    KeyHelp::new("Home / End", "First / last row"),
];

/// The key help for the list, or for the diff while one is open.
fn key_help(diff_open: bool) -> &'static [KeyHelp] {
    if diff_open { DIFF_KEYS } else { KEYS }
}

/// The web app's `DIFF_KEYS`.
const DIFF_KEYS: &[KeyHelp] = &[
    KeyHelp::new(
        "n / p",
        "Next / previous file (an image file opens as images)",
    ),
    KeyHelp::new("j / k", "Scroll down / up"),
    KeyHelp::new("Home / End", "First / last file"),
    KeyHelp::new(
        "v",
        "Mark the current file as viewed (and go to the next), or unmark it",
    ),
    KeyHelp::new("s", "Split or unified layout"),
    KeyHelp::new("A", "Approve the pull request"),
    KeyHelp::new("R", "Request changes"),
    KeyHelp::new("c", "Comment"),
    KeyHelp::new("x", "Close the pull request…"),
    KeyHelp::new("o", "Open the files on GitHub"),
    KeyHelp::new("y", "Copy the URL"),
    KeyHelp::new(
        "Esc",
        "Close the composer or the image, then back to the list",
    ),
];

const SECTION_KEY: &str = "github.section";
use subspace_framework::core::credentials::GITHUB_ACCOUNT_KEY as ACCOUNT_KEY;
const COLLAPSED_KEY: &str = "github.collapsed";
/// Reload when the dashboard shows again after this long (`STALE_MS`).
const STALE: Duration = Duration::from_secs(5 * 60);
/// Holding `j` does not fetch every item's detail.
const DETAIL_DEBOUNCE: Duration = Duration::from_millis(80);

#[cfg(test)]
mod favourites_tests;
#[cfg(test)]
mod menu_runtime_tests;
#[cfg(test)]
mod refresh_tests;

/// `Section1`…`Section3` pick a pull request section by position.
const SECTION_1: usize = 0;
const SECTION_2: usize = 1;
const SECTION_3: usize = 2;
const SECTION_4: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SectionStatus {
    Idle,
    Loading,
    Failed(String),
}

enum SectionRefresh {
    Automatic,
    Manual,
}

#[derive(Default)]
struct Account {
    sections: HashMap<&'static str, SectionData>,
    status: HashMap<&'static str, SectionStatus>,
    details: HashMap<String, GhItemDetail>,
    // A confirmed local write wins over reads that started before it completed.
    state_writes: HashMap<String, (PrState, Instant)>,
    detail_errors: HashMap<String, String>,
    assignable: HashMap<String, Vec<GhUser>>,
    loaded_at: Option<Instant>,
    reloading_sections: bool,
    /// Sections shown from before a reload or from the disk cache. The list
    /// shows them; other Spaces get nothing from them until a refresh lands.
    unconfirmed: HashSet<&'static str>,
    /// Displayed during a manual refresh, with broker answers waiting for a new read.
    unconfirmed_details: HashSet<String>,
}

impl Account {
    fn apply_section(
        &mut self,
        section: &'static str,
        mut response: subspace_framework::client_github::model::SectionResponse,
        started: Instant,
    ) {
        model::preserve_written_state(&mut response.items, &self.state_writes, started);
        self.sections.insert(
            section,
            SectionData {
                total: response.total,
                items: response.items,
                at: model::now_millis(),
            },
        );
        self.status.insert(section, SectionStatus::Idle);
        self.unconfirmed.remove(section);
    }

    /// Keeps showing every section but stops it answering other Spaces
    /// until a refresh confirms it.
    fn unconfirm(&mut self) {
        self.unconfirmed = self.sections.keys().copied().collect();
    }

    fn begin_reload(&mut self) -> bool {
        // Repeated Reload actions share the current manual refresh.
        if self.reloading_sections {
            return false;
        }
        self.loaded_at = None;
        self.status.clear();
        self.unconfirm();
        self.unconfirmed_details = self.details.keys().cloned().collect();
        true
    }

    fn apply_read_detail(&mut self, mut detail: GhItemDetail, started: Instant) {
        model::preserve_written_state(
            std::slice::from_mut(&mut detail.item),
            &self.state_writes,
            started,
        );
        self.detail_errors.remove(&detail.item.key);
        self.unconfirmed_details.remove(&detail.item.key);
        patch_sections(&mut self.sections, &detail.item);
        self.details.insert(detail.item.key.clone(), detail);
    }

    fn apply_written_state(&mut self, item: &GhItem, fresh: Option<GhItemDetail>) {
        self.state_writes
            .insert(item.key.clone(), (item.state, Instant::now()));
        patch_sections(&mut self.sections, item);
        if let Some(detail) = fresh {
            self.details.insert(item.key.clone(), detail);
        }
        if let Some(detail) = self.details.get_mut(&item.key) {
            detail.item = item.clone();
            detail.merge_target = None;
            detail.viewer_can_review = false;
        }
    }

    fn loading(&self) -> bool {
        self.status
            .values()
            .any(|status| *status == SectionStatus::Loading)
    }
}

struct ComposerState {
    entity: Entity<Composer>,
    key: String,
    mode: Mode,
    /// Whether this draft is being sent. Other writes, such as an
    /// assignment, also set the board's `writing`, but leave the draft free.
    sending: bool,
    _subscription: Subscription,
}

enum MenuTarget {
    Account,
    Assignees(String),
    /// The view menu, which stands in for the navigator's views.
    View,
}

// qual:allow(srp, god_struct) reason: "This retained board owns account selection and its rows and details; all measured methods form one cluster."
pub struct Github {
    focus: FocusHandle,
    weak: WeakEntity<Self>,
    client: Option<Arc<Client>>,
    /// The favourite repositories, which All open lists.
    favourites: favourites::Favourites,
    started: bool,
    accounts: Vec<String>,
    accounts_error: Option<String>,
    account: Option<String>,
    data: HashMap<String, Account>,
    // Read fences outlive the in-memory Account that a verified scope can evict.
    read_generations: HashMap<String, u64>,
    section: &'static str,
    selections: HashMap<&'static str, SharedString>,
    list: ListState,
    search: Rc<ItemSearch>,
    collapsed: Vec<String>,
    /// The item whose detail the panel asked for last.
    detail_key: Option<String>,
    navigation: Option<navigation::Target>,
    detail_generation: u64,
    detail_poll: Option<Task<()>>,
    covered: bool,
    checks_open: HashSet<String>,
    composer: Option<ComposerState>,
    close_confirmation: Option<close_pull_request::Confirmation>,
    merge_confirmation: Option<merge_pull_request::Confirmation>,
    writing: bool,
    menu: Option<(Entity<Menu>, MenuTarget, Subscription)>,
    menu_anchors: MenuAnchors,
    assignee_anchor: Entity<Anchor>,
    assignee_bottom_anchor: Entity<Anchor>,
    diff: Option<(Entity<DiffView>, Subscription)>,
    now: i64,
    _detail_task: Option<Task<()>>,
    _search_selection: Option<Task<()>>,
    search_scope: Option<subspace_framework::dashboard::full_text::Scope>,
    retained_watch: Option<Task<()>>,
    retained_permits: std::collections::HashMap<
        (String, String),
        subspace_framework::dashboard::full_text::ReadPermit,
    >,
    search_generation: u64,
    retained_key: Option<String>,
    detail_prefetch: Option<subspace_framework::dashboard::prefetch::Prefetch>,
    _accounts_task: Option<Task<()>>,
    /// Whether the navigator shows; stored as `github.navigator`.
    navigator_open: bool,
    /// Whether the inspector (the detail panel) shows; stored as
    /// `github.inspector`.
    inspector_open: bool,
    /// The one repository the list shows, if any. Kept in memory only.
    repo_filter: Option<String>,
    /// The workspace's text edge, read at each render for the list's rows.
    text_edge: Pixels,
    /// Pull request summaries other Spaces asked for.
    references: references::References,
    _sign_in: Option<Subscription>,
}

fn error_text(error: &Error) -> String {
    error.to_string()
}

impl Github {
    fn account_login(&self) -> Option<&str> {
        self.account.as_deref()
    }
    fn account_data(&self) -> Option<&Account> {
        let login = self.account.as_deref()?.to_lowercase();
        if self
            .retained_permits
            .iter()
            .any(|((account, _), permit)| account == &login && !permit.valid())
        {
            return None;
        }
        self.data.get(&login)
    }

    fn account_data_mut(&mut self) -> Option<&mut Account> {
        let login = self.account.as_deref()?.to_lowercase();
        Some(self.data.entry(login).or_default())
    }

    fn kind(&self) -> ItemKind {
        section_by_id(self.section).map_or(ItemKind::Pr, |section| section.kind)
    }

    /// The repositories `section` searches: the favourites for All open,
    /// and for the others while Favourites only is on.
    fn section_scope(&self, section: &str) -> Option<Vec<String>> {
        let (list, only) = (&self.favourites.list, self.favourites.only);
        section_by_id(section).and_then(|section| section_repos(section, list, only))
    }

    fn section_items(&self) -> &[GhItem] {
        self.account_data()
            .and_then(|account| account.sections.get(self.section))
            .map_or(&[], |data| data.items.as_slice())
    }

    /// The item `key` in any loaded section of the account.
    fn find_item(&self, key: &str) -> Option<&GhItem> {
        let account = self.account_data()?;
        SECTIONS
            .iter()
            .find_map(|section| {
                account
                    .sections
                    .get(section.id)?
                    .items
                    .iter()
                    .find(|item| item.key == key)
            })
            .or_else(|| account.details.get(key).map(|detail| &detail.item))
    }

    fn selected_item(&self) -> Option<GhItem> {
        let key = self.list.selected_row()?.key.clone();
        self.find_item(&key).cloned()
    }

    /// The item actions work on: the diff's pull request, else the selection.
    fn subject_item(&self, cx: &App) -> Option<GhItem> {
        match &self.diff {
            Some((diff, _)) => Some(diff.read(cx).item.clone()),
            None => self.selected_item(),
        }
    }

    // ---- Rows, search and stored choices ---------------------------------------------

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.now = model::now_seconds();
        let account = self.account.clone().unwrap_or_default();
        let groups = rows::groups(self.section_items(), &account, self.now);
        let mut groups = repo_filter::apply(groups, self.repo_filter.as_deref());
        search_index::append_retained(self, &mut groups);
        navigation::append(self, &mut groups);
        self.list.set_groups(groups);
        // Stored folds apply to the repositories now shown.
        for group in self.list.groups().to_vec() {
            let want = self.collapsed.iter().any(|key| key == group.key.as_ref());
            if self.list.is_folded(&group.key) != want {
                self.list.toggle(&group.key);
            }
        }
        let status = self
            .account_data()
            .and_then(|account| account.status.get(self.section).cloned())
            .unwrap_or(SectionStatus::Idle);
        let has_data = self
            .account_data()
            .is_some_and(|account| account.sections.contains_key(self.section))
            || self.retained_key.is_some()
            || self.navigation.is_some();
        let list_status = match (&self.accounts_error, status, has_data) {
            (Some(error), _, false) => LoadStatus::Failed {
                title: "Cannot load GitHub".into(),
                message: error.clone().into(),
            },
            (_, SectionStatus::Failed(error), true) => LoadStatus::Failed {
                title: "Showing saved results. Refresh failed:".into(),
                message: error.into(),
            },
            (_, SectionStatus::Failed(error), false) => LoadStatus::Failed {
                title: "Cannot load this section".into(),
                message: error.into(),
            },
            (_, _, false) if self.accounts_error.is_none() => LoadStatus::Loading,
            _ => LoadStatus::Ready,
        };
        self.list.set_status(list_status);
        let items: Vec<(subspace_framework::dashboard::SearchDoc, Row)> = self
            .account_data()
            .map(|data| {
                search_items(&data.sections)
                    .into_iter()
                    .map(|(_, item)| (search_doc(item), rows::row(item, &account, self.now)))
                    .collect()
            })
            .unwrap_or_default();
        self.search = Rc::new(ItemSearch::new(items));
        cx.notify();
    }

    /// Keeps `github.collapsed` in step with the folds the list shows.
    fn sync_collapsed(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for group in self.list.groups() {
            let folded = self.list.is_folded(&group.key);
            let stored = self
                .collapsed
                .iter()
                .position(|key| key == group.key.as_ref());
            match (folded, stored) {
                (true, None) => {
                    self.collapsed.push(group.key.to_string());
                    changed = true;
                }
                (false, Some(at)) => {
                    self.collapsed.remove(at);
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            prefs::set_list(cx, COLLAPSED_KEY, &self.collapsed);
        }
    }

    // ---- Loading --------------------------------------------------------------------

    fn start(&mut self, cx: &mut Context<Self>) {
        self.started = true;
        // Saved lists first (G-02). Legacy issue sections are dropped so a
        // cache saved before PRs-only never resurrects Issues.
        if let Some(file) = store::path() {
            for (login, sections) in store::load(&file) {
                let account = self.data.entry(login).or_default();
                account.sections = pr_only(sections);
                account.unconfirm();
            }
        }
        if self.account.is_some() {
            self.rebuild(cx);
        }
        self.load_accounts(cx);
    }

    fn load_accounts(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            self.accounts_error = Some("GitHub is not configured.".into());
            self.rebuild(cx);
            return;
        };
        self.accounts_error = None;
        self._accounts_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { client.accounts() }).await;
            _ = this.update(cx, |this, cx| match result {
                Ok(accounts) => {
                    // Settings may have changed the saved account since this view started.
                    let saved = prefs::get(cx, ACCOUNT_KEY).or_else(|| this.account.clone());
                    let pick = accounts.pick(saved.as_deref());
                    this.accounts = accounts.logins;
                    match pick {
                        Some(login) => this.set_account(login, cx),
                        None => {
                            this.accounts_error =
                                Some("No gh accounts found; run gh auth login.".into());
                            this.rebuild(cx);
                        }
                    }
                }
                Err(error) => {
                    this.accounts_error = Some(error_text(&error));
                    this.rebuild(cx);
                }
            });
        }));
    }

    fn set_account(&mut self, login: String, cx: &mut Context<Self>) {
        self.search_generation = self.search_generation.wrapping_add(1);
        self.detail_prefetch = None;
        self._detail_task = None;
        self.retained_key = None;
        let canonical = self
            .accounts
            .iter()
            .find(|known| same_login(known, &login))
            .cloned()
            .unwrap_or(login);
        prefs::set(cx, ACCOUNT_KEY, canonical.clone());
        let changed = self
            .account
            .as_deref()
            .is_some_and(|old| !same_login(old, &canonical));
        self.account = Some(canonical);
        if changed {
            self.navigation = None;
            self.repo_filter = None;
            // A draft belongs to the account it was started under: never let
            // another account send it.
            self.composer = None;
        }
        references::account_chosen(self, changed, cx);
        search_index::account_changed(self, changed, cx);
        self.list.close_detail();
        self.detail_key = None;
        self.rebuild(cx);
        let stale = self
            .account_data()
            .and_then(|account| account.loaded_at)
            .is_none_or(|at| at.elapsed() > STALE);
        if stale {
            self.load_all(SectionRefresh::Automatic, cx);
        }
        navigation::reveal(self, cx);
    }

    /// Every pull request section of the account, in parallel, so the tab
    /// counts and the item search fill in. Issue sections are never loaded:
    /// this board shows pull requests only (REQ-02).
    fn load_all(&mut self, refresh: SectionRefresh, cx: &mut Context<Self>) {
        let (Some(client), Some(login)) = (self.client.clone(), self.account.clone()) else {
            return;
        };
        let account = self.data.entry(login.to_lowercase()).or_default();
        if account.loading() {
            return;
        }
        account.reloading_sections = matches!(refresh, SectionRefresh::Manual);
        account.loaded_at = Some(Instant::now());
        references::refreshing(self);
        for section in sections_of(ItemKind::Pr) {
            self.load_section(client.clone(), login.clone(), section.id, cx);
        }
        self.rebuild(cx);
    }

    fn load_section(
        &mut self,
        client: Arc<Client>,
        login: String,
        section: &'static str,
        cx: &mut Context<Self>,
    ) {
        let account_key = login.to_lowercase();
        let account = self.data.entry(account_key.clone()).or_default();
        account.status.insert(section, SectionStatus::Loading);
        let generation = self.read_generation(&account_key);
        let repos = self.section_scope(section);
        let started = Instant::now();
        let since = references::since(self);
        let clock = references::clock(self);
        cx.spawn(async move |this, cx| {
            let fetch_login = login.clone();
            // The section's tick is taken as its request starts.
            let (tick, result) = cx
                .background_spawn(async move {
                    let tick = clock.tick();
                    (
                        tick,
                        client.section(&fetch_login, section, repos.as_deref()),
                    )
                })
                .await;
            _ = this.update(cx, |this, cx| {
                if this.read_generation(&account_key) != generation {
                    return;
                }
                let account = this.data.entry(account_key).or_default();
                let loaded = result.is_ok();
                match result {
                    Ok(response) => account.apply_section(section, response, started),
                    Err(error) => {
                        account
                            .status
                            .insert(section, SectionStatus::Failed(error_text(&error)));
                    }
                }
                account.reloading_sections &= account.loading();
                if loaded {
                    this.save_cache(cx);
                    references::section_landed(this, &login, (section, tick), since, cx);
                }
                search_index::start_prefetch(this, cx);
                this.rebuild(cx);
            });
        })
        .detach();
    }

    fn read_generation(&self, account_key: &str) -> u64 {
        self.read_generations
            .get(account_key)
            .copied()
            .unwrap_or_default()
    }

    fn invalidate_for_reload(&mut self) {
        let Some(account_key) = self.account.as_ref().map(|login| login.to_lowercase()) else {
            return;
        };
        if self
            .data
            .get_mut(&account_key)
            .is_none_or(Account::begin_reload)
        {
            self.advance_read_generation(account_key);
            references::reset_reads(self);
        }
    }

    fn advance_read_generation(&mut self, account_key: String) {
        let generation = self.read_generations.entry(account_key).or_default();
        *generation = generation.wrapping_add(1);
    }

    fn save_cache(&self, cx: &mut Context<Self>) {
        let Some(file) = store::path() else {
            return;
        };
        let snapshot: HashMap<String, store::AccountCache> = self
            .data
            .iter()
            .filter(|(_, account)| !account.sections.is_empty())
            .map(|(login, account)| (login.clone(), account.sections.clone()))
            .collect();
        cx.background_spawn(async move {
            _ = store::save(&file, &snapshot);
        })
        .detach();
    }

    /// Fetch the open item's detail, after a short pause, unless the cached
    /// detail is as new as the row.
    fn ensure_detail(&mut self, cx: &mut Context<Self>) {
        let open = match &self.diff {
            Some((diff, _)) => self
                .composer
                .as_ref()
                .map(|_| diff.read(cx).item.key.clone()),
            None => self.visible_detail_key().map(str::to_owned),
        };
        let Some(key) = open else {
            self.detail_key = None;
            return;
        };
        if self.detail_key.as_deref() == Some(key.as_str()) {
            return;
        }
        self.detail_key = Some(key.clone());
        let fresh = self.find_item(&key).is_some_and(|item| {
            self.account_data()
                .and_then(|account| account.details.get(&key))
                .is_some_and(|detail| detail.item.updated_at == item.updated_at)
        });
        if !fresh {
            self.fetch_detail(key, true, cx);
        }
    }

    fn visible_detail_key(&self) -> Option<&str> {
        if self.diff.is_some() {
            return None;
        }
        self.list
            .open_row()
            .or_else(|| self.list.selected_row().filter(|_| self.inspector_open))
            .map(|row| row.key.as_ref())
    }

    fn start_detail_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.detail_poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(25))
                    .await;
                if this
                    .update_in(cx, |board, window, cx| {
                        if window.is_visible() && !board.covered {
                            board.poll_visible_detail(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn poll_visible_detail(&mut self, cx: &mut Context<Self>) {
        if self.writing
            || self
                ._detail_task
                .as_ref()
                .is_some_and(|task| !task.is_ready())
        {
            return;
        }
        if let Some(key) = self.visible_detail_key().map(str::to_owned) {
            self.fetch_detail(key, false, cx);
        }
    }

    fn fetch_detail(&mut self, key: String, debounce: bool, cx: &mut Context<Self>) {
        let (Some(client), Some(login)) = (self.client.clone(), self.account.clone()) else {
            return;
        };
        if let Some(account) = self.account_data_mut() {
            account.detail_errors.remove(&key);
        }
        search_index::load(
            self,
            search_index::Request {
                client,
                login,
                key,
                debounce,
                since: references::since(self),
                clock: references::clock(self),
            },
            cx,
        );
    }

    // ---- Sections and accounts ------------------------------------------------------

    fn go_section(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if self.diff.is_some() || !is_pr_section(id) {
            return;
        }
        if let Some(selected) = self.list.selected() {
            self.selections.insert(self.section, selected.clone());
        }
        self.navigation = None;
        self.section = id;
        prefs::set(cx, SECTION_KEY, id);
        self.list.close_detail();
        self.rebuild(cx);
        if let Some(key) = self.selections.get(id).cloned()
            && self.list.visible_keys().contains(&key)
        {
            self.list.select(key);
        }
    }

    fn section_action(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(section) = SECTIONS
            .get(index)
            .filter(|section| section.kind == ItemKind::Pr)
        {
            self.go_section(section.id, cx);
        }
    }

    /// The view menu's pick: the section with id `id`, or a repository to
    /// filter to.
    fn go_view(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(repo) = id.strip_prefix(repo_filter::MENU_PREFIX) {
            self.pick_repository(repo, cx);
        } else if let Some(section) = section_by_id(id) {
            self.go_section(section.id, cx);
        }
    }

    fn switch_account(&mut self, _: &SwitchAccount, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_account_at(None, window, cx);
    }

    fn switch_account_at(
        &mut self,
        anchor: Option<Entity<Anchor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.diff.is_some() {
            return;
        }
        if self.accounts.len() < 2 {
            emit_toast(Toast::info("Only one GitHub account is signed in."), cx);
            return;
        }
        let current = self.account.clone().unwrap_or_default();
        let options = self
            .accounts
            .iter()
            .map(|login| {
                MenuOption::new(login.clone(), login.clone()).checked(same_login(login, &current))
            })
            .collect();
        self.open_menu_at(
            MenuRequest {
                label: "Account",
                placeholder: "Switch account",
                options,
                target: MenuTarget::Account,
            },
            anchor,
            window,
            cx,
        );
    }

    fn open_menu_at(
        &mut self,
        request: MenuRequest,
        anchor: Option<Entity<Anchor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let MenuRequest {
            label,
            placeholder,
            options,
            target,
        } = request;
        let (label, placeholder) = (label.to_string(), placeholder.to_string());
        let id = match &target {
            MenuTarget::Account => "gh-account",
            MenuTarget::Assignees(_) => "gh-assign",
            MenuTarget::View => layout::VIEW_ANCHOR,
        };
        let anchor = anchor.unwrap_or_else(|| self.menu_anchors.anchor(id, cx));
        let Some(menu) = self.menu_anchors.open(
            anchor,
            MenuContents::new(label, placeholder, options),
            window,
            cx,
        ) else {
            return;
        };
        let subscription = cx.subscribe_in(&menu, window, |this, _, event, _, cx| {
            let target = this.menu.take().map(|(_, target, _)| target);
            if let (MenuEvent::Pick(value), Some(target)) = (event, target) {
                match target {
                    MenuTarget::Account => this.set_account(value.to_string(), cx),
                    MenuTarget::Assignees(key) => this.toggle_assignee(key, value.to_string(), cx),
                    MenuTarget::View => this.go_view(value, cx),
                }
            }
            cx.notify();
        });
        self.menu = Some((menu, target, subscription));
        cx.notify();
    }

    fn collapse_group(&mut self, _: &CollapseGroup, _: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.list.selected().cloned() else {
            return;
        };
        if let Some(group) = self.list.groups().iter().find(|g| g.key == selected) {
            let key = group.key.clone();
            if !self.list.is_folded(&key) {
                self.list.toggle(&key);
            }
        } else if let Some(group) = self
            .list
            .groups()
            .iter()
            .find(|g| g.rows.iter().any(|row| row.key == selected))
        {
            let key = group.key.clone();
            list::update(self, cx, |list| list.select(key));
        }
        cx.notify();
    }

    fn expand_group(&mut self, _: &ExpandGroup, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(selected) = self.list.selected().cloned()
            && self.list.groups().iter().any(|g| g.key == selected)
            && self.list.is_folded(&selected)
        {
            self.list.toggle(&selected);
            cx.notify();
        }
    }

    // ---- Writes ---------------------------------------------------------------------

    fn modes_for(&self, item: &GhItem) -> Vec<Mode> {
        let detail = self.account_data().and_then(|a| a.details.get(&item.key));
        let mut modes = Vec::new();
        if can_review(item, detail) {
            modes.extend([Mode::Approve, Mode::Changes]);
        }
        if can_comment(detail) {
            modes.push(Mode::Comment);
        }
        modes
    }

    fn compose(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.subject_item(cx) else {
            emit_toast(Toast::error("Select an item first"), cx);
            return;
        };
        let detail = self.account_data().and_then(|a| a.details.get(&item.key));
        if matches!(mode, Mode::Approve | Mode::Changes) && !can_review(&item, detail) {
            let message = if item.kind == ItemKind::Issue {
                "Issues have no reviews: press c to comment"
            } else {
                "You cannot review this pull request"
            };
            emit_toast(Toast::error(message), cx);
            return;
        }
        if mode == Mode::Comment && !can_comment(detail) {
            emit_toast(Toast::error("You cannot comment here"), cx);
            return;
        }
        if self.keep_draft(&item.key, mode, window, cx) {
            return;
        }
        // The composer takes the inspector's footer, so the inspector shows.
        if self.diff.is_none() {
            self.set_inspector(true, cx);
        }
        let entity = cx.new(|cx| {
            let mut composer = Composer::new(mode.title(), mode.placeholder(), window, cx);
            configure(&mut composer, mode, window, cx);
            composer
        });
        entity.update(cx, |composer, cx| composer.focus(window, cx));
        let subscription =
            cx.subscribe_in(&entity, window, |this, _, event, window, cx| match event {
                ComposerEvent::Submit(text) => this.submit(text.clone(), cx),
                ComposerEvent::Cancel => {
                    this.composer = None;
                    this.focus.focus(window, cx);
                    cx.notify();
                }
            });
        self.composer = Some(ComposerState {
            entity,
            key: item.key.clone(),
            mode,
            sending: false,
            _subscription: subscription,
        });
        self.ensure_detail(cx);
        cx.notify();
    }

    /// Whether an open composer keeps its draft instead of a new one
    /// replacing it. On the same pull request the composer switches to
    /// `mode` and keeps its text; a draft on another pull request stays, and
    /// a toast says where it is.
    fn keep_draft(
        &mut self,
        key: &str,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = &self.composer else {
            return false;
        };
        // A draft being sent keeps its mode until GitHub answers. Another
        // write in flight, such as an assignment, leaves the draft free.
        if open.sending {
            return true;
        }
        if open.key == key {
            if self.diff.is_none() {
                self.set_inspector(true, cx);
            }
            self.set_mode(mode, window, cx);
            return true;
        }
        if open.entity.read(cx).text(cx).trim().is_empty() {
            return false;
        }
        let message = format!("Send or cancel the draft on {} first", open.key);
        emit_toast(Toast::error(message), cx);
        true
    }

    fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(composer) = &mut self.composer else {
            return;
        };
        composer.mode = mode;
        composer.entity.update(cx, |composer, cx| {
            configure(composer, mode, window, cx);
            composer.focus(window, cx);
        });
        cx.notify();
    }

    fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        let (Some(client), Some(login)) = (self.client.clone(), self.account.clone()) else {
            return;
        };
        let Some(composer) = &self.composer else {
            return;
        };
        if self.writing {
            return;
        }
        let (key, mode, entity) = (composer.key.clone(), composer.mode, composer.entity.clone());
        let body = text.trim().to_string();
        if mode == Mode::Changes && body.is_empty() {
            entity.update(cx, |composer, cx| {
                composer.set_error("Text is required.", cx)
            });
            return;
        }
        self.start_sending();
        entity.update(cx, |composer, cx| composer.set_busy(true, cx));
        let number = model::split_key(&key).map_or(0, |(_, number)| number);
        cx.spawn(async move |this, cx| {
            let (write_key, writer) = (key.clone(), login.clone());
            let result = cx
                .background_spawn(async move {
                    let login = writer;
                    match mode {
                        Mode::Approve => {
                            client.review(&login, &write_key, ReviewEvent::Approve, &body)
                        }
                        Mode::Changes => {
                            client.review(&login, &write_key, ReviewEvent::RequestChanges, &body)
                        }
                        Mode::Comment => client.comment(&login, &write_key, &body),
                    }
                })
                .await;
            _ = this.update(cx, |this, cx| {
                this.finish_sending(&entity);
                match result {
                    Ok(detail) => {
                        this.apply_detail(detail, &login, cx);
                        this.close_sent_composer(&entity);
                        emit_toast(Toast::info(written_message(mode, number)), cx);
                    }
                    Err(error) => {
                        entity.update(cx, |composer, cx| {
                            composer.set_error(error_text(&error), cx);
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Marks the board as writing and the open draft as the one sending.
    fn start_sending(&mut self) {
        self.writing = true;
        if let Some(open) = self.composer.as_mut() {
            open.sending = true;
        }
    }

    /// Ends the send once GitHub has answered, freeing the composer that
    /// sent if it is still open.
    fn finish_sending(&mut self, sent: &Entity<Composer>) {
        self.writing = false;
        if let Some(open) = self
            .composer
            .as_mut()
            .filter(|open| open.entity.entity_id() == sent.entity_id())
        {
            open.sending = false;
        }
    }

    /// Closes the composer that sent, but not one opened since, as under
    /// another account.
    fn close_sent_composer(&mut self, sent: &Entity<Composer>) {
        if self
            .composer
            .as_ref()
            .is_some_and(|open| open.entity.entity_id() == sent.entity_id())
        {
            self.composer = None;
        }
    }

    /// Fresh detail from a write replaces the panel and the rows (REQ-62),
    /// in the data of `login`, the account that wrote, even if the selected
    /// account has changed since.
    fn apply_detail(&mut self, detail: GhItemDetail, login: &str, cx: &mut Context<Self>) {
        let item = model::row_of(&detail);
        if self.detail_key.as_deref() == Some(&item.key)
            && self
                .account
                .as_deref()
                .is_some_and(|active| same_login(active, login))
        {
            self.detail_generation = self.detail_generation.wrapping_add(1);
            self._detail_task = None;
        }
        let account = self.data.entry(login.to_lowercase()).or_default();
        patch_sections(&mut account.sections, &item);
        account.details.insert(item.key.clone(), detail);
        self.save_cache(cx);
        references::announce(cx);
        self.rebuild(cx);
    }

    fn assign(&mut self, _: &Assign, window: &mut Window, cx: &mut Context<Self>) {
        self.assign_at(None, window, cx);
    }

    fn assign_at(
        &mut self,
        anchor: Option<Entity<Anchor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.diff.is_some() {
            return;
        }
        let Some(item) = self.selected_item() else {
            emit_toast(Toast::error("Select an item first"), cx);
            return;
        };
        let (Some(client), Some(login)) = (self.client.clone(), self.account.clone()) else {
            return;
        };
        let cached = self
            .account_data()
            .and_then(|account| account.assignable.get(&item.repo.to_lowercase()).cloned());
        if let Some(users) = cached {
            self.list.reveal(&item.key);
            let request = self.assignee_menu(&item, &users);
            self.open_menu_at(request, anchor, window, cx);
            return;
        }
        let repo = item.repo.clone();
        cx.spawn_in(window, async move |this, cx| {
            let fetch_repo = repo.clone();
            let result = cx
                .background_spawn(async move { client.assignable(&login, &fetch_repo) })
                .await;
            _ = this.update_in(cx, |this, window, cx| match result {
                Ok(response) => {
                    if let Some(account) = this.account_data_mut() {
                        account
                            .assignable
                            .insert(repo.to_lowercase(), response.users.clone());
                    }
                    this.list.reveal(&item.key);
                    let request = this.assignee_menu(&item, &response.users);
                    this.open_menu_at(request, anchor, window, cx);
                }
                Err(error) => emit_toast(
                    Toast::error(format!("Cannot load people for {repo}: {error}")),
                    cx,
                ),
            });
        })
        .detach();
    }

    fn assignee_menu(&self, item: &GhItem, assignable: &[GhUser]) -> MenuRequest {
        let account = self.account.clone().unwrap_or_default();
        let options = assignee_logins(&item.assignees, &account, assignable)
            .into_iter()
            .map(|user| {
                let assigned = item
                    .assignees
                    .iter()
                    .any(|known| same_login(&known.login, &user.login));
                MenuOption::new(
                    user.login.clone(),
                    detail::person_label(&user.login, user.name.as_deref()),
                )
                .checked(assigned)
            })
            .collect();
        MenuRequest {
            label: "Assignees",
            placeholder: "Assign someone",
            options,
            target: MenuTarget::Assignees(item.key.clone()),
        }
    }

    fn assign_me(&mut self, _: &AssignMe, _: &mut Window, cx: &mut Context<Self>) {
        if self.diff.is_some() {
            return;
        }
        let (Some(item), Some(login)) = (self.selected_item(), self.account.clone()) else {
            emit_toast(Toast::error("Select an item first"), cx);
            return;
        };
        self.toggle_assignee(item.key, login, cx);
    }

    /// Adds `user` to the item's assignees, or removes them.
    fn toggle_assignee(&mut self, key: String, user: String, cx: &mut Context<Self>) {
        let (Some(client), Some(login)) = (self.client.clone(), self.account.clone()) else {
            return;
        };
        let Some(item) = self.find_item(&key).cloned() else {
            return;
        };
        if self.writing {
            return;
        }
        let assigned = item
            .assignees
            .iter()
            .any(|known| same_login(&known.login, &user));
        self.writing = true;
        cx.spawn(async move |this, cx| {
            let (write_key, write_user, writer) = (key.clone(), user.clone(), login.clone());
            let result = cx
                .background_spawn(async move {
                    let (add, remove) = if assigned {
                        (Vec::new(), vec![write_user])
                    } else {
                        (vec![write_user], Vec::new())
                    };
                    client.set_assignees(&writer, &write_key, &add, &remove)
                })
                .await;
            _ = this.update(cx, |this, cx| {
                this.writing = false;
                let number = item.number;
                let short = short_login(&user).to_string();
                match result {
                    Ok(detail) => {
                        this.apply_detail(detail, &login, cx);
                        let message = if assigned {
                            format!("Unassigned {short} from #{number}")
                        } else {
                            format!("Assigned {short} to #{number}")
                        };
                        emit_toast(Toast::info(message), cx);
                    }
                    Err(error) => {
                        emit_toast(Toast::error(format!("Assignees not changed: {error}")), cx)
                    }
                }
            });
        })
        .detach();
    }

    // ---- Diff, open and copy --------------------------------------------------------

    fn show_diff(&mut self, _: &ShowDiff, window: &mut Window, cx: &mut Context<Self>) {
        if self.diff.is_some() {
            return;
        }
        let Some(item) = self.selected_item() else {
            emit_toast(Toast::error("Select an item first"), cx);
            return;
        };
        self.open_diff(item, window, cx);
    }

    fn open_diff(&mut self, item: GhItem, window: &mut Window, cx: &mut Context<Self>) {
        if item.kind != ItemKind::Pr {
            emit_toast(Toast::error("Issues have no files"), cx);
            return;
        }
        let Some(login) = self.account.clone() else {
            return;
        };
        let focus = self.focus.clone();
        let client = self.client.clone();
        let open = OpenDiff {
            item,
            login,
            client,
            focus,
        };
        let view = cx.new(|cx| DiffView::new(open, window, cx));
        let subscription =
            cx.subscribe_in(&view, window, |this, _, event, window, cx| match event {
                DiffEvent::Close => this.close_diff(window, cx),
                DiffEvent::Toast(toast) => emit_toast(toast.clone(), cx),
            });
        self.composer = None;
        self.diff = Some((view, subscription));
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn close_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer.take().is_some() {
            cx.notify();
            return;
        }
        self.diff = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn open_in_github(&mut self, _: &OpenInGithub, _: &mut Window, cx: &mut Context<Self>) {
        match self.subject_item(cx) {
            Some(item) => {
                links::open(&item.url, cx);
            }
            None => emit_toast(Toast::error("Select an item first"), cx),
        }
    }

    fn copy_url(&mut self, _: &CopyUrl, _: &mut Window, cx: &mut Context<Self>) {
        match self.subject_item(cx) {
            Some(item) => {
                links::copy(item.url.clone(), cx);
                emit_toast(Toast::info(format!("Copied {}", item.url)), cx);
            }
            None => emit_toast(Toast::error("Select an item first"), cx),
        }
    }

    fn panel_actions(&self) -> OnAction {
        let weak = self.weak.clone();
        Rc::new(move |action, window, cx| {
            _ = weak.update(cx, |this, cx| match action {
                PanelAction::Compose(mode) => this.compose(mode, window, cx),
                PanelAction::Close => close_pull_request::open(this, window, cx),
                PanelAction::Assign(place) => {
                    let anchor = match place {
                        "bottom" => this.assignee_bottom_anchor.clone(),
                        _ => this.assignee_anchor.clone(),
                    };
                    this.assign_at(Some(anchor), window, cx);
                }
                PanelAction::Diff => {
                    if let Some(item) = this.selected_item() {
                        this.open_diff(item, window, cx);
                    }
                }
                PanelAction::Open => this.open_in_github(&OpenInGithub, window, cx),
                PanelAction::ToggleChecks => {
                    if let Some(key) = this.detail_key.clone()
                        && !this.checks_open.remove(&key)
                    {
                        this.checks_open.insert(key);
                    }
                    cx.notify();
                }
            });
        })
    }

    /// The composer element with its mode switch, for item `key`.
    fn composer_element(&self, key: &str, tokens: &Tokens) -> Option<AnyElement> {
        let composer = self.composer.as_ref().filter(|c| c.key == key)?;
        let item = self.find_item(key).cloned();
        let modes = item
            .as_ref()
            .map(|item| self.modes_for(item))
            .unwrap_or_default();
        let weak = self.weak.clone();
        let switch = detail::mode_switch(&modes, composer.mode, tokens, move |mode, window, cx| {
            _ = weak.update(cx, |this, cx| this.set_mode(mode, window, cx));
        });
        Some(
            v_flex()
                .w_full()
                .min_w_0()
                .children(switch)
                .child(composer.entity.clone())
                .into_any_element(),
        )
    }

    fn panel_input<'a>(&'a self, item: &'a GhItem, tokens: &Tokens) -> PanelInput<'a> {
        let account = self.account_data();
        PanelInput {
            item,
            detail: account.and_then(|a| a.details.get(&item.key)),
            error: account.and_then(|a| a.detail_errors.get(&item.key).map(String::as_str)),
            account: self.account_login().unwrap_or_default(),
            checks_open: self.checks_open.contains(&item.key),
            writing: self.writing,
            composer: self.composer_element(&item.key, tokens),
            now: self.now,
            assignee_anchor: self.assignee_anchor.clone(),
            assignee_bottom_anchor: self.assignee_bottom_anchor.clone(),
        }
    }
}

impl EventEmitter<DashboardEvent> for Github {}

fn configured_client(cx: &App) -> Option<Arc<Client>> {
    services(cx).map(|services| {
        let client = Client::new(services.credentials.clone())
            .with_work_account(prefs::get(cx, ACCOUNT_KEY));
        Arc::new(client)
    })
}

impl Dashboard for Github {
    const ID: &'static str = "github";
    const SEARCHABLE: bool = true;
    const TITLE: &'static str = "PRs";
    const ICON: Option<&'static str> = Some("logos/github.svg");
    const ARTWORK: subspace_framework::dashboard::BoardArtwork =
        subspace_framework::dashboard::BoardArtwork::new(&[(
            "logos/github.svg",
            include_bytes!("../assets/logos/github.svg"),
        )])
        .with_order(10);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn settings() -> subspace_framework::dashboard::settings::BoardSettings {
        subspace_framework::dashboard::settings::BoardSettings {
            description: "Shows the pull requests you opened, are asked to review or are involved in, and every open pull request in your favourite GitHub repositories. You can inspect checks, comments and diffs, then comment, assign or review a pull request. Issues do not appear in this board.",
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

    fn contracts() -> subspace_framework::dashboard::contracts::BoardContracts {
        use subspace_framework::contracts::github::OpenPullRequest;
        use subspace_framework::contracts::github::{AccessChanged, Changed, ResolvePullRequests};
        use subspace_framework::dashboard::contracts::{BoardContracts, Declaration};
        const CONTRACTS: BoardContracts = BoardContracts::new(&[
            Declaration::responds::<OpenPullRequest>(),
            Declaration::responds::<ResolvePullRequests>(),
            Declaration::publishes::<Changed>(),
            Declaration::publishes::<AccessChanged>(),
        ]);
        CONTRACTS
    }

    fn open_link(&mut self, url: &str, cx: &mut Context<Self>) -> bool {
        navigation::open(self, url, cx)
    }

    fn agent_tools(
        environment: &subspace_framework::dashboard::ToolEnvironment,
    ) -> Vec<Arc<dyn subspace_framework::dashboard::AgentTool>> {
        agent_tools::agent_tools(environment)
    }

    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let section = prefs::get(cx, SECTION_KEY)
            .filter(|id| is_pr_section(id))
            .and_then(|id| section_by_id(&id))
            .map_or(default_section(ItemKind::Pr), |section| section.id);
        let menu_anchors = MenuAnchors::default();
        let assignee_anchor = menu_anchors.anchor("gh-assign", cx);
        let assignee_bottom_anchor = menu_anchors.anchor("gh-assign-bottom", cx);
        Self {
            focus: cx.focus_handle(),
            weak: cx.entity().downgrade(),
            client: configured_client(cx),
            favourites: favourites::Favourites::new(cx),
            started: false,
            accounts: Vec::new(),
            accounts_error: None,
            account: prefs::get(cx, ACCOUNT_KEY),
            data: HashMap::new(),
            read_generations: HashMap::new(),
            section,
            selections: HashMap::new(),
            list: ListState::default(),
            search: Rc::new(ItemSearch::new([])),
            collapsed: prefs::get_list(cx, COLLAPSED_KEY),
            detail_key: None,
            navigation: None,
            detail_generation: 0,
            detail_poll: None,
            covered: false,
            checks_open: HashSet::new(),
            composer: None,
            close_confirmation: None,
            merge_confirmation: None,
            writing: false,
            menu: None,
            menu_anchors,
            assignee_anchor,
            assignee_bottom_anchor,
            diff: None,
            now: model::now_seconds(),
            _detail_task: None,
            _search_selection: None,
            search_scope: None,
            retained_watch: None,
            retained_permits: Default::default(),
            search_generation: 0,
            retained_key: None,
            detail_prefetch: None,
            _accounts_task: None,
            navigator_open: layout::dock_open(cx, layout::NAVIGATOR_KEY),
            inspector_open: layout::dock_open(cx, layout::INSPECTOR_KEY),
            repo_filter: None,
            text_edge: layout::current_text_edge(cx),
            references: Default::default(),
            _sign_in: references::attach(cx),
        }
    }

    fn bind_keys(cx: &mut App) {
        let context = view_keys(Self::ID);
        let context = Some(context.as_str());
        cx.bind_keys([
            KeyBinding::new("1", Section1, context),
            KeyBinding::new("2", Section2, context),
            KeyBinding::new("3", Section3, context),
            KeyBinding::new("4", Section4, context),
            KeyBinding::new("u", SwitchAccount, context),
            KeyBinding::new("shift-f", OpenRepositories, context),
            KeyBinding::new("f", ToggleFavourite, context),
            KeyBinding::new("shift-a", Approve, context),
            KeyBinding::new("shift-r", RequestChanges, context),
            KeyBinding::new("c", Comment, context),
            KeyBinding::new("x", ClosePullRequest, context),
            KeyBinding::new("a", Assign, context),
            KeyBinding::new("i", AssignMe, context),
            KeyBinding::new("d", ShowDiff, context),
            KeyBinding::new("o", OpenInGithub, context),
            KeyBinding::new("y", CopyUrl, context),
            KeyBinding::new("left", CollapseGroup, context),
            KeyBinding::new("right", ExpandGroup, context),
        ]);
        diff::view::bind_keys(cx);
        close_pull_request::bind_keys(cx);
        merge_pull_request::bind_keys(cx);
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_detail_poll(window, cx);
        if !self.started {
            self.start(cx);
            return;
        }
        let stale = self
            .account_data()
            .and_then(|account| account.loaded_at)
            .is_some_and(|at| at.elapsed() > STALE);
        if stale {
            self.load_all(SectionRefresh::Automatic, cx);
        }
    }

    fn background_refresh(&mut self, cx: &mut Context<Self>) {
        if !subspace_framework::dashboard::refresh::enabled(Self::ID, cx) {
            return;
        }
        let interval =
            subspace_framework::dashboard::refresh::interval(Self::ID, REFRESH_INTERVAL, cx);
        let recent_or_loading = self.account_data().is_some_and(|account| {
            account.loading() || account.loaded_at.is_some_and(|at| at.elapsed() < interval)
        });
        if recent_or_loading {
            return;
        }
        if !self.started {
            self.start(cx);
        } else if self.account.is_none() {
            self.load_accounts(cx);
        } else {
            self.load_all(SectionRefresh::Automatic, cx);
        }
    }

    fn covered(&mut self, covered: bool, _window: &mut Window, _cx: &mut Context<Self>) {
        self.covered = covered;
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.detail_poll = None;
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
        self.favourites.close();
        self.detail_prefetch = None;
    }

    fn docks(&self, _cx: &App) -> SpaceDocks {
        self.space_docks()
    }

    fn toggle_dock(&mut self, dock: SpaceDock, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_space_dock(dock, cx);
    }

    fn status(&self, cx: &App) -> Option<SharedString> {
        if let Some((diff, _)) = &self.diff {
            return Some(format!("{} · files", diff.read(cx).key()).into());
        }
        let counts = self.view_counts();
        let line = layout::status_line(&self.scope_line(), counts.as_deref());
        Some(repo_filter::with_note(line, self.repo_filter.as_deref()).into())
    }

    fn busy(&self, _cx: &App) -> bool {
        self.account_data().is_some_and(|account| {
            account
                .status
                .values()
                .any(|status| *status == SectionStatus::Loading)
        })
    }

    fn subject(&self, cx: &App) -> Option<Row> {
        if let Some((diff, _)) = &self.diff {
            let item = diff.read(cx).item.clone();
            return Some(rows::row(
                &item,
                self.account_login().unwrap_or_default(),
                self.now,
            ));
        }
        self.list.selected_row().cloned()
    }

    fn agent_context(&self, cx: &App) -> Option<subspace_framework::dashboard::AgentContext> {
        let item = self.subject_item(cx)?;
        let full = self.account_data().and_then(|a| a.details.get(&item.key));
        let detail = detail::detail(&item, full, self.now);
        Some(agent_context::item_context(&item, &detail))
    }

    fn commands(&self, cx: &Context<Self>) -> Vec<PaletteCommand> {
        let mut commands = self.account_commands();
        commands.extend(self.place_commands(cx));
        commands.extend(self.item_commands(cx));
        commands
    }

    fn search(&self, _cx: &App) -> Option<Search> {
        Some(Search {
            placeholder: "Search pull requests and issues".into(),
            items: self.search.clone(),
        })
    }

    fn open_search_result(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.diff.is_some() {
            self.composer = None;
            self.close_diff(window, cx);
        }
        let holds = |this: &Self, section: &str| {
            this.account_data()
                .and_then(|account| account.sections.get(section))
                .is_some_and(|data| data.items.iter().any(|item| item.key == id))
        };
        if !holds(self, self.section)
            && let Some(section) = SECTIONS.iter().find(|section| holds(self, section.id))
        {
            self.go_section(section.id, cx);
        }
        list::update(self, cx, |list| {
            list.reveal(id);
        });
        cx.notify();
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

    fn modal(&self, _cx: &App) -> Option<Modal> {
        if self.close_confirmation.is_some() || self.merge_confirmation.is_some() {
            return Some(Modal::Dialog);
        }
        (self.menu.is_some() || self.favourites.is_open()).then_some(Modal::Menu)
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        close_pull_request::cancel(self, window, cx);
        merge_pull_request::cancel(self, window, cx);
        self.menu_anchors.dismiss(window, cx);
        self.favourites.close();
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    fn keys(&self) -> &'static [KeyHelp] {
        key_help(self.diff.is_some())
    }

    fn help_foot(&self) -> Option<SharedString> {
        Some(if self.diff.is_some() {
            "/ searches the files. Tree keys stay in the tree.".into()
        } else {
            "Lists show saved results first and refresh from GitHub.".into()
        })
    }

    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.started {
            self.start(cx);
            return;
        }
        self.invalidate_for_reload();
        self._detail_task = None;
        if self.account.is_none() || self.accounts.is_empty() {
            self.load_accounts(cx);
            return;
        }
        if let Some((diff, _)) = &self.diff {
            diff.update(cx, |diff, cx| diff.load(cx));
        } else {
            self.load_all(SectionRefresh::Manual, cx);
        }
        if let Some(key) = self.detail_key.clone() {
            self.fetch_detail(key, false, cx);
        }
    }
}

impl ListDashboard for Github {
    /// Rows start at the workspace's text edge, as the navigator's and the
    /// toolbar's content do.
    fn list_side_inset(&self) -> Option<Pixels> {
        Some(self.text_edge)
    }

    fn comfortable_rows(&self) -> bool {
        true
    }

    fn on_selection_changed(&mut self, selected: Option<SharedString>, cx: &mut Context<Self>) {
        navigation::selection_changed(self, selected.as_deref(), cx);
    }

    /// Closing the page drops a pull request opened from another Space.
    fn on_panel_toggled(&mut self, open: bool, cx: &mut Context<Self>) {
        if !open {
            navigation::clear(self, cx);
        }
    }

    /// One click on a pull request only selects it, which fills the
    /// inspector; a double click, Enter or the inspector's Open opens its
    /// page. Repository headers still fold on one click.
    fn activates_on_click(&self, _row: &Row) -> bool {
        false
    }

    /// The row's state glyph, as the list draws it, with an id the tests
    /// can click.
    fn row_leading(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        let tokens = Tokens::current(cx);
        Some(
            div()
                .id(SharedString::from(format!("gh-row:{}", row.key)))
                .test_support()
                .flex_shrink_0()
                .text_color(tokens.glyph(&row.icon))
                .child(row.icon.text.clone())
                .into_any_element(),
        )
    }

    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    fn detail(&self, key: &str) -> Option<Detail> {
        let Some(item) = self.find_item(key) else {
            return navigation::pending_detail(self, key);
        };
        let full = self.account_data().and_then(|a| a.details.get(key));
        Some(detail::detail(item, full, self.now))
    }

    fn empty_message(&self) -> (SharedString, SharedString) {
        let (title, hint) = model::empty_message(self.section, self.favourites.scope());
        (title.into(), hint.into())
    }

    /// The row's page on GitHub, so the menu can open and copy it.
    fn row_menu(&self, row: &Row) -> RowMenu {
        let facts = RowMenu::from_row(row);
        match self.find_item(row.key.as_ref()) {
            Some(item) => facts.link("GitHub", &item.url),
            None => facts,
        }
    }

    /// A repository header stands for its repository: its page, and the star.
    fn group_menu(&self, group: &Group) -> Option<RowMenu> {
        let repo = group.key.strip_prefix("repo:")?;
        Some(
            RowMenu::new(repo)
                .link("GitHub", &format!("https://github.com/{repo}"))
                .opens_details(false),
        )
    }

    /// The star for the repository of the line the menu is for, in the Act
    /// group, with its key.
    fn row_commands(board: &Entity<Self>, cx: &App) -> Vec<MenuCommand> {
        let this = board.read(cx);
        let Some(repo) = this.selected_repo() else {
            return Vec::new();
        };
        let label = favourites::toggle_label(&repo, this.favourites.contains(&repo));
        let weak = board.downgrade();
        let star = MenuCommand::new(MenuGroup::Act, label, IconName::Star, move |_, cx| {
            _ = weak.update(cx, |this, cx| this.toggle_favourite(repo.clone(), cx));
        });
        vec![star.key(ToggleFavourite)]
    }

    fn group_indicators(&self, group: &Group, _cx: &App) -> Vec<GroupIndicator> {
        match group.key.strip_prefix("repo:") {
            Some(repo) if self.favourites.contains(repo) => vec![GroupIndicator {
                icon: IconName::StarFill,
                tone: subspace_framework::dashboard::tokens::Tone::Accent,
                label: "Favourite repository".into(),
                tooltip: "Favourite repository".into(),
            }],
            _ => Vec::new(),
        }
    }

    /// All open, or Favourites only, without favourites: add some.
    fn empty_actions(&self, cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        if !model::needs_favourites(self.section, self.favourites.scope()) {
            return None;
        }
        let add = subspace_framework::dashboard::action::action_button(
            "gh-empty-add-repositories",
            subspace_framework::dashboard::action::Intent::Neutral,
            Some(IconName::Star),
            "Add repositories…",
        )
        .on_click(cx.listener(|this, _, window, cx| this.open_repositories(None, window, cx)));
        Some(vec![add.into_any_element()])
    }
}

impl Focusable for Github {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Github {
    /// The panel over the diff view while the composer is open.
    fn diff_panel(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let item = self.composed_diff_item(cx)?;
        let tokens = Tokens::current(cx);
        let full = self.account_data().and_then(|a| a.details.get(&item.key));
        let detail = detail::detail(&item, full, self.now);
        let body = gpui_kit::component::v_flex()
            .child(detail::props(&detail, &tokens))
            .child(detail::panel_extra(
                self.panel_input(&item, &tokens),
                &tokens,
                self.panel_actions(),
            ));
        let header = panel::PanelHeader {
            icon: Some(rows::state_glyph(&item)),
            kind: detail.kind,
            id: detail.id,
            label: None,
            link: detail.link,
        };
        Some(panel::frame(
            header,
            body,
            panel::PanelActions::new(self.close_composer(), None),
            window,
            cx,
        ))
    }

    /// The diff view beside the composer panel, if one is open.
    fn diff_root(
        &mut self,
        diff: Entity<DiffView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let panel = self.diff_panel(window, cx);
        let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
        context.add(Self::ID);
        h_flex()
            .id("github-diff-root")
            .key_context(context)
            .size_full()
            .min_h_0()
            .child(div().flex_1().min_w_0().h_full().child(diff))
            .children(panel)
    }

    /// The list view: the workspace, which holds the navigator and the list
    /// with its tab, toolbar and list (or the open pull request's page), and
    /// the inspector. The root carries the Space's key context and focus
    /// around all of them, so keys answer from the navigator, the page and
    /// the inspector as well as from the list.
    fn list_root(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let workspace = self.workspace(window, cx);
        let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
        context.add(DASHBOARD_CONTEXT);
        context.add(Self::ID);
        let root = v_flex()
            .id("github-list-root")
            .key_context(context)
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .child(div().flex_1().min_h_0().child(workspace));
        page::with_page_keys(root, cx)
    }

    /// The account context at the start of the status line.
    fn scope_line(&self) -> String {
        self.signed().scope_line()
    }

    /// The current view's counts, such as `8 open PRs · 4 repos`, once its
    /// list has loaded. Both counts are the whole view's: a repository
    /// filter is named after them, not counted into them.
    fn view_counts(&self) -> Option<String> {
        let data = self.account_data()?.sections.get(self.section)?;
        let noun = match self.kind() {
            ItemKind::Pr => "open PRs",
            ItemKind::Issue => "open issues",
        };
        let repos = model::group_by_repo(&data.items).len();
        let counts = layout::counts_line(data.items.len(), data.total, repos, noun);
        Some(counts)
    }

    /// The item of the diff view, when the composer is open on it.
    fn composed_diff_item(&self, cx: &App) -> Option<GhItem> {
        let (diff, _) = self.diff.as_ref()?;
        let item = diff.read(cx).item.clone();
        match self.composer.as_ref()?.key == item.key {
            true => Some(item),
            false => None,
        }
    }

    /// Closes the composer over the diff view and returns the focus.
    fn close_composer(&self) -> impl Fn(&mut Window, &mut App) + 'static {
        let weak = self.weak.clone();
        move |window: &mut Window, cx: &mut App| {
            _ = weak.update(cx, |this, cx| {
                this.composer = None;
                this.focus.focus(window, cx);
                cx.notify();
            });
        }
    }
}

/// The toast after a review or comment on pull request `number` is sent.
fn written_message(mode: Mode, number: u32) -> String {
    match mode {
        Mode::Approve => format!("Approved #{number}"),
        Mode::Changes => format!("Requested changes on #{number}"),
        Mode::Comment => format!("Commented on #{number}"),
    }
}

/// Sets the composer up for `mode`: label, placeholder, submit label, and
/// whether text is required. An approval may be empty.
fn configure(composer: &mut Composer, mode: Mode, window: &mut Window, cx: &mut Context<Composer>) {
    composer.set_label(mode.title(), cx);
    composer.set_placeholder(mode.placeholder(), window, cx);
    composer.set_submit_label(mode.title(), cx);
    composer.set_optional(mode == Mode::Approve, cx);
}

impl Render for Github {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        search_index::purge(self, cx);
        self.sync_collapsed(cx);
        self.ensure_detail(cx);
        let menu = self.menu_anchors.view();
        let close_dialog = close_pull_request::render(self, cx);
        let merge_dialog = merge_pull_request::render(self, cx);
        let root = match self.diff.as_ref().map(|(diff, _)| diff.clone()) {
            Some(diff) => self.diff_root(diff, window, cx),
            None => self.list_root(window, cx),
        };
        root.on_action(cx.listener(|this, _: &Section1, _, cx| this.section_action(SECTION_1, cx)))
            .on_action(cx.listener(|this, _: &Section2, _, cx| this.section_action(SECTION_2, cx)))
            .on_action(cx.listener(|this, _: &Section3, _, cx| this.section_action(SECTION_3, cx)))
            .on_action(cx.listener(|this, _: &Section4, _, cx| this.section_action(SECTION_4, cx)))
            .on_action(cx.listener(Self::switch_account))
            .on_action(cx.listener(|this, _: &OpenRepositories, window, cx| {
                if this.diff.is_none() {
                    this.open_repositories(None, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleFavourite, _, cx| {
                this.toggle_selected_favourite(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFavouritesOnly, _, cx| {
                this.toggle_favourites_only(cx);
            }))
            .on_action(cx.listener(|this, _: &ClearRepositoryFilter, _, cx| {
                this.clear_repository_filter(cx);
            }))
            .on_action(cx.listener(|this, _: &Approve, window, cx| {
                this.compose(Mode::Approve, window, cx);
            }))
            .on_action(cx.listener(|this, _: &RequestChanges, window, cx| {
                this.compose(Mode::Changes, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Comment, window, cx| {
                this.compose(Mode::Comment, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClosePullRequest, window, cx| {
                close_pull_request::open(this, window, cx);
            }))
            .on_action(cx.listener(Self::assign))
            .on_action(cx.listener(Self::assign_me))
            .on_action(cx.listener(Self::show_diff))
            .on_action(cx.listener(Self::open_in_github))
            .on_action(cx.listener(Self::copy_url))
            .on_action(cx.listener(Self::collapse_group))
            .on_action(cx.listener(Self::expand_group))
            .children(menu)
            .children(close_dialog)
            .children(merge_dialog)
    }
}

/// The glyph of a view, in the navigator and on the view menu.
fn github_section_icon(section: &str) -> IconName {
    match section {
        "mine" => IconName::UserRound,
        "review" => IconName::Eye,
        "involved" => IconName::UsersRound,
        "open" => IconName::GitPullRequest,
        "assigned" => IconName::UserCheck,
        "created" => IconName::CirclePlus,
        _ => IconName::AtSign,
    }
}

/// The command palette's entries.
impl Github {
    /// Switching account, when there is more than one.
    fn account_commands(&self) -> Vec<PaletteCommand> {
        if self.accounts.len() < 2 {
            return Vec::new();
        }
        let weak = self.weak.clone();
        let current = self.account.clone().unwrap_or_default();
        let options = self
            .accounts
            .iter()
            .map(|login| {
                PaletteOption::new(login.clone(), login.clone())
                    .checked(same_login(login, &current))
            })
            .collect();
        vec![
            PaletteCommand::sub(
                "Switch account…",
                SubList::new("Account", options, move |login, _, cx| {
                    let login = login.to_string();
                    _ = weak.update(cx, |this, cx| this.set_account(login, cx));
                }),
            )
            .keywords("account user login gh")
            .shortcut("u"),
        ]
    }

    /// The commands of the diff view, or of the list.
    fn place_commands(&self, cx: &Context<Self>) -> Vec<PaletteCommand> {
        match self.diff.is_some() {
            true => self.diff_commands(cx),
            false => self.list_commands(),
        }
    }

    fn diff_commands(&self, cx: &Context<Self>) -> Vec<PaletteCommand> {
        use diff::view as dv;
        vec![
            PaletteCommand::new("Next file", dv::NextFile).shortcut("n"),
            PaletteCommand::new("Previous file", dv::PreviousFile).shortcut("p"),
            PaletteCommand::new("Toggle viewed on the current file", dv::ToggleViewed)
                .keywords("mark seen")
                .shortcut("v"),
            PaletteCommand::new(self.layout_command_label(cx), dv::ToggleLayout)
                .keywords("layout side by side")
                .shortcut("s"),
            PaletteCommand::new("Search files", dv::SearchFiles)
                .keywords("tree filter find")
                .shortcut("/"),
            PaletteCommand::new("Back to the list", dv::DiffEscape).shortcut("Esc"),
        ]
    }

    /// The layout command names the layout it switches to.
    fn layout_command_label(&self, cx: &Context<Self>) -> &'static str {
        match self.diff.as_ref().map(|(d, _)| d.read(cx).layout()) {
            Some(diff::model::Layout::Split) => "Unified diff",
            _ => "Split diff",
        }
    }

    /// One command per pull request section (REQ-02: PRs only), then the
    /// favourites.
    fn list_commands(&self) -> Vec<PaletteCommand> {
        let mut commands: Vec<PaletteCommand> = SECTIONS
            .iter()
            .enumerate()
            .filter(|(_, section)| section.kind == ItemKind::Pr)
            .map(|(index, section)| self.section_command(index, section))
            .collect();
        commands.extend(self.favourite_commands());
        commands
    }

    fn favourite_commands(&self) -> Vec<PaletteCommand> {
        let only = match self.favourites.only {
            true => "Show all repositories in Mine, Review requested and Involved",
            false => "Show favourites only in Mine, Review requested and Involved",
        };
        let mut commands = vec![
            PaletteCommand::new("Repositories…", OpenRepositories)
                .keywords("favourite favorites star search repos")
                .shortcut("F"),
            PaletteCommand::new(only, ToggleFavouritesOnly).keywords("favourite filter scope"),
        ];
        if let Some(repo) = &self.repo_filter {
            commands.push(
                PaletteCommand::new(
                    format!("Show all repositories, not only {repo}"),
                    ClearRepositoryFilter,
                )
                .keywords("repository filter clear all"),
            );
        }
        if let Some(repo) = self.selected_repo() {
            let label = favourites::toggle_label(&repo, self.favourites.contains(&repo));
            commands.push(
                PaletteCommand::new(label, ToggleFavourite)
                    .keywords("favourite star repository")
                    .shortcut("f"),
            );
        }
        commands
    }

    fn section_command(&self, index: usize, section: &SectionDef) -> PaletteCommand {
        let current = if section.id == self.section {
            " (current)"
        } else {
            ""
        };
        let keywords = match section.kind {
            ItemKind::Pr => "section pull requests prs",
            ItemKind::Issue => "section issues",
        };
        PaletteCommand {
            label: format!("Go to {}{current}", section.title).into(),
            keywords: keywords.into(),
            shortcut: Some((index + 1).to_string().into()),
            run: subspace_framework::dashboard::CommandRun::Action(section_action_for(index)),
            recent_key: None,
        }
    }

    /// The commands that act on the selected item.
    fn item_commands(&self, cx: &Context<Self>) -> Vec<PaletteCommand> {
        let Some(item) = self.subject_item(cx) else {
            return Vec::new();
        };
        let detail = self.account_data().and_then(|a| a.details.get(&item.key));
        let mut commands = review_commands(can_review(&item, detail), can_comment(detail));
        if close_pull_request::is_closable(&item) {
            commands.push(
                PaletteCommand::new("Close pull request…", ClosePullRequest)
                    .keywords("close closed")
                    .shortcut("x"),
            );
        }
        if self.diff.is_some() {
            commands.extend(diff_item_commands());
        } else {
            commands.extend(self.list_item_commands(&item));
        }
        commands
    }

    /// Assigning, the diff and the links of an item in the list.
    fn list_item_commands(&self, item: &GhItem) -> Vec<PaletteCommand> {
        let me = self.account.clone().unwrap_or_default();
        let mine = item
            .assignees
            .iter()
            .any(|user| same_login(&user.login, &me));
        let mut commands = vec![
            PaletteCommand::new("Assign…", Assign)
                .keywords("assignee people")
                .shortcut("a"),
            PaletteCommand::new(if mine { "Unassign me" } else { "Assign to me" }, AssignMe)
                .keywords("assignee self")
                .shortcut("i"),
        ];
        if item.kind == ItemKind::Pr {
            commands.push(
                PaletteCommand::new("Files: show the diff", ShowDiff)
                    .keywords("diff changes code")
                    .shortcut("d"),
            );
        }
        commands.push(
            PaletteCommand::new("Open in GitHub", OpenInGithub)
                .keywords("browser web")
                .shortcut("o"),
        );
        commands.push(
            PaletteCommand::new("Copy URL", CopyUrl)
                .keywords("link clipboard")
                .shortcut("y"),
        );
        commands
    }
}

/// The action that the section at `index` runs.
fn section_action_for(index: usize) -> Box<dyn Action> {
    match index {
        SECTION_1 => Box::new(Section1),
        SECTION_2 => Box::new(Section2),
        SECTION_3 => Box::new(Section3),
        _ => Box::new(Section4),
    }
}

/// Approving, requesting changes and commenting, when the item allows them.
fn review_commands(review: bool, comment: bool) -> Vec<PaletteCommand> {
    let mut commands = Vec::new();
    if review {
        commands.push(
            PaletteCommand::new("Approve", Approve)
                .keywords("review lgtm")
                .shortcut("A"),
        );
        commands.push(
            PaletteCommand::new("Request changes", RequestChanges)
                .keywords("review reject")
                .shortcut("R"),
        );
    }
    if comment {
        commands.push(
            PaletteCommand::new("Comment", Comment)
                .keywords("reply write")
                .shortcut("c"),
        );
    }
    commands
}

/// The links of the item the diff view shows.
fn diff_item_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand::new("Open the files on GitHub", diff::view::OpenFiles)
            .keywords("browser web")
            .shortcut("o"),
        PaletteCommand::new("Copy URL", diff::view::CopyUrl)
            .keywords("link clipboard")
            .shortcut("y"),
    ]
}

/// What `open_menu` needs to show one menu.
struct MenuRequest {
    label: &'static str,
    placeholder: &'static str,
    options: Vec<MenuOption>,
    target: MenuTarget,
}

fn emit_toast(toast: Toast, cx: &mut Context<Github>) {
    cx.emit(DashboardEvent::Toast(toast));
}

#[cfg(test)]
mod tests {
    use super::key_help;

    #[test]
    fn help_lists_include_close_and_keep_the_navigation_hints() {
        let (list, diff) = (key_help(false), key_help(true));
        assert_eq!(list.len(), 18);
        assert_eq!(diff.len(), 12);
        assert_eq!(list[3].key, "1 – 4");
        assert_eq!(diff[0].key, "n / p");
        assert!(list.iter().any(|key| key.key == "x"));
        assert!(diff.iter().any(|key| key.key == "x"));
    }
}
