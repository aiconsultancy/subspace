//! The board's own changes: choices from the tabs, toolbar and keys, the
//! reads they ask for, and the list lines drawn from the listing.

use std::collections::HashMap;
use std::time::Instant;

use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, SharedString, Window};
use subspace_api_client::model::IssueListItem;
use subspace_framework::dashboard::{ListState, prefs};

use crate::Issues;
use crate::lines::{self, Folds, Line};
use crate::listing::{Listing, More, Status};
use crate::model::{IssuePriority, ProjectScope, SubIssues, Tab, chosen_project};
use crate::requests::Request;
use crate::scope::{self, Access};

mod search;
pub(crate) use search::SearchFallback;

/// Selecting a row this close to its group's end reads the next page.
const NEAR_END_ROWS: usize = 3;

const TAB_PREF: &str = "issues.tab";
const SUB_ISSUES_PREF: &str = "issues.subissues";
const FOLDED_PREF: &str = "issues.folded";
const UNFOLDED_PREF: &str = "issues.unfolded";

pub(crate) const PROJECT_PREF: &str = "issues.project";

pub(crate) fn new(window: &mut Window, cx: &mut Context<Issues>) -> Issues {
    let (search, typing) = search_input(window, cx);
    let (initiatives, initiative_id, initiative_changes) = crate::initiative::connect(window, cx);
    Issues {
        editor: None,
        project_move: None,
        authoring: crate::authoring::AuthoringView::new(window),
        back: Default::default(),
        weak: cx.entity().downgrade(),
        focus: cx.focus_handle(),
        list: ListState::default(),
        activity: Default::default(),
        issue_view: Default::default(),
        media: Default::default(),
        decision: None,
        project: initial_project(initiative_id.as_deref(), cx),
        profile: "subspace-api".to_owned(),
        access: Access::Starting,
        projects: Vec::new(),
        initiatives,
        initiative_id,
        tab: prefs::get(cx, TAB_PREF)
            .and_then(|key| Tab::from_key(&key))
            .unwrap_or_default(),
        sub_issues: prefs::get(cx, SUB_ISSUES_PREF)
            .map(|key| SubIssues::from_key(&key))
            .unwrap_or_default(),
        filters: Default::default(),
        search,
        query: String::new(),
        search_fallback: SearchFallback::default(),
        listing: Listing::default(),
        index: HashMap::new(),
        folds: Folds {
            folded: prefs::get_list(cx, FOLDED_PREF),
            unfolded: prefs::get_list(cx, UNFOLDED_PREF),
        },
        search_folds: Default::default(),
        search_pages: Default::default(),
        requests: Vec::new(),
        hub: None,
        hub_id: None,
        seen: None,
        syncing: false,
        hidden_selection: None,
        profile_label: None,
        live: Default::default(),
        poll: None,
        hub_changes: None,
        menu_anchors: Default::default(),
        menu: None,
        width: f32::default(),
        _search_task: None,
        _subscriptions: vec![typing, initiative_changes],
    }
}

fn initial_project(initiative: Option<&str>, cx: &gpui_kit::App) -> ProjectScope {
    if initiative.is_some() {
        return ProjectScope::AllProjects;
    }
    chosen_project(crate::initiative::saved_project(cx), &[])
}

fn search_input(
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> (gpui_kit::Entity<InputState>, gpui_kit::Subscription) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search issues"));
    let typing = cx.subscribe(&input, search::changed);
    (input, typing)
}

// qual:allow(coupling, oi) reason: "The board's state changes live beside its constructor, one file per concern of the board: lib.rs holds the shell traits, seam.rs what Subspace API's answers change."
impl Issues {
    /// The hub's name for its profile, such as *Subspace API fixture*, else the
    /// offered profile's.
    pub(crate) fn profile_name(&self) -> &str {
        self.profile_label.as_deref().unwrap_or("Subspace API")
    }

    pub(crate) fn is_ready(&self) -> bool {
        matches!(self.access, Access::Ready { .. })
    }

    /// Pass list choices to the hub, which reads them once the connection is ready.
    pub(crate) fn read_list(&mut self, cx: &mut Context<Self>) {
        if crate::initiative::no_initiative_projects(self) {
            return;
        }
        if !self.is_ready() && self.hub.is_none() {
            return;
        }
        if self.is_ready() {
            self.listing.start_read();
        }
        self.request(Request::List(self.choice()), cx);
        self.sync_lines(cx);
    }

    /// A new scope starts from nothing, so no other list shows under it.
    pub(crate) fn rescope(&mut self, cx: &mut Context<Self>) {
        self.cancel_search_fallback();
        self.reset_scope(cx);
    }

    pub(crate) fn clear_list_scope(&mut self) {
        self.search_pages.clear();
        self.listing.clear();
    }

    fn reset_scope(&mut self, cx: &mut Context<Self>) {
        self.clear_list_scope();
        self.read_visible(cx);
        self.sync_lines(cx);
    }

    pub(crate) fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let leaving_activity = std::mem::take(&mut self.activity.visible);
        if self.tab == tab {
            self.cancel_search_fallback();
            if leaving_activity {
                self.read_list(cx);
            }
            return;
        }
        self.filters.clear_status();
        if self.tab == Tab::MyIssues || tab == Tab::MyIssues {
            self.filters.assignee = None;
        }
        self.tab = tab;
        prefs::set(cx, TAB_PREF, tab.key());
        self.rescope(cx);
    }

    pub(crate) fn set_project(&mut self, project: ProjectScope, cx: &mut Context<Self>) {
        if self.project == project || !crate::initiative::project_in_scope(self, &project) {
            return;
        }
        prefs::set(cx, PROJECT_PREF, project.key());
        self.set_list_project(project);
        self.rescope(cx);
    }

    pub(crate) fn set_list_project(&mut self, project: ProjectScope) {
        self.project = project;
        self.activity.clear();
        self.filters.clear_status();
    }

    pub(crate) fn toggle_sub_issues(&mut self, cx: &mut Context<Self>) {
        self.sub_issues = self.sub_issues.toggled();
        prefs::set(cx, SUB_ISSUES_PREF, self.sub_issues.key());
        self.rescope(cx);
    }

    /// The issue a row key names.
    pub(crate) fn issue(&self, key: &str) -> Option<&IssueListItem> {
        let (group, row) = *self.index.get(key)?;
        self.listing
            .content
            .as_ref()?
            .groups
            .get(group)?
            .rows
            .get(row)
    }

    /// A row's accessible name: identifier, state, priority and title, so the
    /// icon-only lanes are never the only way to read them.
    pub(crate) fn row_label(&self, row: &subspace_framework::dashboard::Row) -> SharedString {
        match self.issue(&row.key) {
            Some(item) => format!(
                "{}, {}, {}, {}",
                item.fields.identifier,
                item.fields.state.name,
                IssuePriority::from_value(item.fields.priority).look().label,
                item.fields.title
            )
            .into(),
            None => row.title.clone(),
        }
    }

    /// Redraws the list lines from the listing, keeping folds and selection.
    pub(crate) fn sync_lines(&mut self, cx: &mut Context<Self>) {
        let groups = self
            .listing
            .content
            .as_ref()
            .map(|content| content.groups.clone())
            .unwrap_or_default();
        self.index = row_index(&groups);
        self.list.set_groups(lines::list_groups(&groups));
        for group in &groups {
            let key = lines::group_key(&group.key);
            if self.list.is_folded(&key) != self.group_folded(group) {
                self.list.toggle(&key);
            }
        }
        let status = scope::list_status(&self.listing, Instant::now());
        self.list.set_status(status);
        cx.notify();
    }

    /// Enter or a click opens an issue, toggles a fold or asks for more rows.
    pub(crate) fn activate_line(&mut self, key: &SharedString, cx: &mut Context<Self>) -> bool {
        match lines::line(key) {
            Some(Line::Group(group)) => {
                let group = group.to_owned();
                self.fold_changing(&group, cx);
                true
            }
            Some(Line::More(group)) => {
                let group = group.to_owned();
                self.read_more(&group, cx);
                true
            }
            Some(Line::Issue(_)) => {
                self.open_row(key, cx);
                true
            }
            None => true,
        }
    }

    /// Folds or unfolds a group once and saves the choice, reading an
    /// unfolded group's rows the first time. The board makes the fold
    /// itself: that read can redraw the lines with the saved fold first.
    fn fold_changing(&mut self, group: &str, cx: &mut Context<Self>) {
        let key = lines::group_key(group);
        let folding = !self.list.is_folded(&key);
        let ask = self.listing.status != Status::Loading;
        if let Some(found) = self.listing.group_mut(group) {
            let kind = found.kind;
            let first_read = !folding && ask && found.more == More::Unread;
            if first_read {
                found.more = More::Loading;
            }
            self.record_fold(group, kind, folding, cx);
            if first_read {
                self.listing.asked_first_page(group);
                let group = group.to_owned();
                self.request(Request::Group { group }, cx);
            }
        }
        if self.list.is_folded(&key) != folding {
            self.list.toggle(&key);
        }
    }

    /// Reads a group's next page when it waits or failed.
    pub(crate) fn read_more(&mut self, group: &str, cx: &mut Context<Self>) {
        let Some(found) = self.listing.group_mut(group) else {
            return;
        };
        if !matches!(found.more, More::Available | More::Failed) {
            return;
        }
        found.more = More::Loading;
        let group = group.to_owned();
        self.request(Request::More { group }, cx);
        self.sync_lines(cx);
    }

    /// Selecting a row near its group's end reads the next page.
    pub(crate) fn load_more_near(
        &mut self,
        selected: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let Some((group, row)) = selected.and_then(|key| self.index.get(&key).copied()) else {
            return;
        };
        let near = self.listing.content.as_ref().and_then(|content| {
            let found = content.groups.get(group)?;
            (found.more == More::Available && row + NEAR_END_ROWS >= found.rows.len())
                .then(|| found.key.clone())
        });
        if let Some(key) = near {
            self.read_more(&key, cx);
        }
    }
}

/// Each issue row's key, with its group and position.
fn row_index(groups: &[crate::listing::IssueGroup]) -> HashMap<SharedString, (usize, usize)> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(group, found)| {
            found
                .rows
                .iter()
                .enumerate()
                .map(move |(row, item)| (lines::issue_key(&item.fields.id), (group, row)))
        })
        .collect()
}
