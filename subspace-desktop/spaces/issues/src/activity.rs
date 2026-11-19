//! Activity is a separate projection of the issue hub. It retains its own
//! selection and uses the same read-only issue page as the grouped list.

mod rows;

use std::collections::HashMap;
use std::time::Instant;

use gpui_kit::{AnyElement, App, Context, SharedString, Window};
use subspace_api_client::ActivityQuery;
use subspace_api_client::hub::{ActivitySnapshot, HubSnapshot};
use subspace_framework::dashboard::{ListState, Row, RowMenu};

use crate::Issues;
use crate::model::ProjectScope;
use crate::requests::Request;
use rows::{Entry, Intent};

#[derive(Default)]
pub(crate) struct Feed {
    pub(crate) visible: bool,
    pub(crate) list: ListState,
    query: Option<ActivityQuery>,
    rows: HashMap<SharedString, Entry>,
    loading: bool,
    refreshed: Option<Instant>,
}

impl Feed {
    fn open(&mut self) {
        self.visible = true;
    }

    pub(crate) fn clear(&mut self) {
        self.query = None;
        self.drop_content();
    }

    pub(crate) fn drop_content(&mut self) {
        self.rows.clear();
        self.list = ListState::default();
        self.loading = false;
        self.refreshed = None;
    }

    fn query(&mut self, project: &ProjectScope) -> ActivityQuery {
        self.query
            .get_or_insert_with(|| {
                let project = match project {
                    ProjectScope::Project(key) => Some(key.as_str()),
                    ProjectScope::AllProjects => None,
                };
                ActivityQuery::default().project(project)
            })
            .clone()
    }

    fn apply(&mut self, shown: Option<&ActivitySnapshot>) {
        if !self.visible {
            return;
        }
        self.loading =
            shown.is_none_or(|view| view.trackers().loading() || view.recent().loading());
        self.refreshed =
            shown.and_then(|view| view.trackers().updated_at().max(view.recent().updated_at()));
        let [trackers, recent] = rows::SECTIONS
            .map(|section| rows::section(section, shown.map(|view| view.section(section))));
        let (tracker_group, tracker_rows) = trackers;
        let (recent_group, recent_rows) = recent;
        self.rows = tracker_rows.into_iter().chain(recent_rows).collect();
        self.list.set_groups(vec![tracker_group, recent_group]);
    }

    pub(crate) fn busy(&self) -> bool {
        self.loading
    }

    pub(crate) fn status(&self, profile: &str, now: Instant) -> String {
        if self.loading {
            return format!("{profile} · Loading activity…");
        }
        self.refreshed.map_or_else(
            || format!("{profile} · Activity"),
            |at| {
                format!(
                    "{profile} · Activity · {}",
                    crate::listing::updated(at, now)
                )
            },
        )
    }

    pub(crate) fn leading(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        self.rows.get(&row.key)?.leading(cx)
    }

    pub(crate) fn trailing(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        self.rows.get(&row.key)?.trailing(cx)
    }

    pub(crate) fn label(&self, row: &Row) -> SharedString {
        self.rows
            .get(&row.key)
            .map_or_else(|| row.title.clone(), |entry| entry.accessible(row))
    }

    pub(crate) fn row_menu(&self, row: &Row) -> RowMenu {
        rows::row_menu(row, self.rows.get(&row.key))
    }

    pub(crate) fn issue_identifier(&self, key: &str) -> Option<&str> {
        match &self.rows.get(key)?.intent {
            Intent::Issue(identifier) => Some(identifier),
            _ => None,
        }
    }

    pub(crate) fn action_name(&self, key: &str) -> Option<SharedString> {
        match self.rows.get(key)?.intent {
            Intent::Issue(_) => Some("Open issue".into()),
            Intent::Page(..) => self
                .list
                .rows()
                .find(|row| row.key == key)
                .map(|row| row.title.clone()),
            Intent::None => None,
        }
    }

    fn adjacent(&self, forward: bool) -> Option<SharedString> {
        let keys = self.list.visible_keys();
        let index = keys
            .iter()
            .position(|key| Some(key) == self.list.selected())?;
        if forward {
            keys[index + 1..]
                .iter()
                .find(|key| self.issue_identifier(key).is_some())
        } else {
            keys[..index]
                .iter()
                .rev()
                .find(|key| self.issue_identifier(key).is_some())
        }
        .cloned()
    }
}

// qual:allow(coupling, oi) reason: "Activity has its own view state and request lifecycle beside the grouped list and issue detail."
impl Issues {
    pub(crate) fn open_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_search_fallback();
        self.activity.open();
        self.focus_list(window, cx);
        self.read_activity(false, cx);
    }

    pub(crate) fn read_visible(&mut self, cx: &mut Context<Self>) {
        crate::initiative::clear_empty_scope(self);
        if self.activity.visible {
            self.read_activity(false, cx);
        } else {
            self.read_list(cx);
        }
    }

    pub(crate) fn read_activity(&mut self, reload: bool, cx: &mut Context<Self>) {
        if reload && (self.initiative_id.is_some() || !self.is_ready()) {
            self.request(Request::ReloadProjects, cx);
        }
        if crate::initiative::no_initiative_projects(self) {
            cx.notify();
            return;
        }
        let query = self.activity.query(&self.project);
        self.request(Request::Activity { query, reload }, cx);
        cx.notify();
    }

    pub(crate) fn follow_activity(&mut self, hub: &HubSnapshot) {
        if crate::initiative::no_initiative_projects(self) {
            return;
        }
        let shown = hub.activity().filter(|view| {
            self.activity.query.as_ref() == Some(view.query()) && hub.profile_id() == self.profile
        });
        self.activity.apply(shown);
    }

    pub(crate) fn activate_activity_row(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.activity.rows.get(key) else {
            return;
        };
        match &entry.intent {
            Intent::Issue(identifier) => self.open_detail(identifier.clone(), cx),
            Intent::Page(section, more) => self.request(
                Request::ActivityPage {
                    section: *section,
                    more: *more,
                },
                cx,
            ),
            Intent::None => {}
        }
    }

    pub(crate) fn step_activity(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(next) = self.activity.adjacent(forward) else {
            return;
        };
        self.activity.list.select(next.clone());
        self.activate_activity_row(&next, cx);
    }
}
