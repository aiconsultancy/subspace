//! The board and the hub. The board reads the hub's snapshot only when its
//! version changes and turns requests into hub calls.

use std::time::Instant;

use gpui_kit::Context;

use crate::Issues;
use crate::link::{self, Link};
use crate::listing::{Failure, IssueGroup, More, Status};
use crate::requests::Request;
use crate::scope::Access;
use crate::sync::{self, HubView};

/// A snapshot applied can ask for reads that change the version again; a
/// few rounds settle it.
const MAX_PULLS: usize = 4;

mod catalogue;
mod dispatch;

// qual:allow(coupling, oi) reason: "The board's link to the hub is its own concern, kept apart from the board's own choices in board.rs."
impl Issues {
    /// Finds the running hub, or notices it stopped, then applies a changed
    /// snapshot.
    pub(crate) fn poll_hub(&mut self, cx: &mut Context<Self>) {
        if link::current(cx).is_some_and(|link| link.hub.account_changed()) {
            link::stop(cx);
            link::start(cx);
        }
        let link = link::current(cx);
        if link.as_ref().map(|link| link.id) != self.hub_id {
            self.attach(link, cx);
            if self.hub_changes.is_some() {
                self.observe_responder(cx);
            }
        }
        self.pull_snapshot(cx);
    }

    fn attach(&mut self, link: Option<Link>, cx: &mut Context<Self>) {
        self.cancel_search_fallback();
        self.editor = None;
        self.issue_view = Default::default();
        // The page goes, and with it where its Back went: after an account
        // change that may be a view, such as a decision, of the old account.
        // The arrival stays, as a link opening now polls the hub first and
        // the shell has just named the view it came from.
        self.back.closed();
        self.activity.clear();
        self.hub_id = link.as_ref().map(|link| link.id);
        self.hub = link.map(|link| link.hub);
        self.seen = None;
        self.listing.clear();
        if self.hub.is_none() {
            self.forget(cx);
            return;
        }
        // A NETT hub restarts for another organisation with the same user,
        // so its initiatives are read again here rather than on a new identity.
        if self
            .hub
            .as_ref()
            .is_some_and(|hub| hub.service() == subspace_api_client::Backend::Nett)
        {
            crate::initiative::refresh(self, cx);
        }
        self.read_visible(cx);
    }

    /// Drop hidden content; the retained hub rebuilds it on activation and
    /// restores the selected issue.
    pub(crate) fn hide_content(&mut self, cx: &mut Context<Self>) {
        self.cancel_search_fallback();
        self.close_detail(cx);
        self.media.clear();
        self.hidden_selection = self.list.selected().cloned();
        self.listing.drop_content();
        self.activity.drop_content();
        self.seen = None;
        self.sync_lines(cx);
    }

    /// Selects again the line selected before the board was hidden, when
    /// it is back.
    pub(crate) fn restore_selection(&mut self) {
        let Some(key) = self.hidden_selection.take() else {
            return;
        };
        if self.list.visible_keys().contains(&key) {
            self.list.select(key);
        }
    }

    /// Without a hub nothing read stays.
    fn forget(&mut self, cx: &mut Context<Self>) {
        self.profile_label = None;
        self.access = Access::Starting;
        self.listing.clear();
        self.sync_lines(cx);
    }

    /// Reload and `r`: the list and the projects, read again.
    pub(crate) fn reload_list(&mut self, cx: &mut Context<Self>) {
        if crate::initiative::no_initiative_projects(self) {
            self.request(Request::ReloadProjects, cx);
            return;
        }
        self.cancel_search_fallback();
        if !self.is_ready() {
            return self.retry(cx);
        }
        self.search_pages.clear();
        self.listing.start_read();
        self.request(Request::Reload(self.choice()), cx);
        self.sync_lines(cx);
    }

    /// Retry the visible list or reconnect the API.
    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
        if self.not_a_member {
            crate::initiative::refresh(self, cx);
            return;
        }
        if crate::initiative::no_initiative_projects(self) {
            self.request(Request::ReloadProjects, cx);
            return;
        }
        if self.activity.visible {
            return self.read_activity(true, cx);
        }
        if self.is_ready() {
            return self.reload_list(cx);
        }
        self.request(Request::Access, cx);
        cx.notify();
    }

    /// Activation reads the list again when its last read is over a minute
    /// old (spec section 7). The hub keeps the same rule.
    pub(crate) fn read_if_stale(&mut self, cx: &mut Context<Self>) {
        if self.hub.is_some() || self.listing.is_stale(Instant::now()) {
            self.read_visible(cx);
        }
    }

    /// Follows the retained hub's existing timer while the board shows.
    pub(crate) fn start_polling(&mut self, cx: &mut Context<Self>) {
        self.hub_changes = Some(link::observe(cx));
        self.observe_responder(cx);
    }

    fn observe_responder(&mut self, cx: &mut Context<Self>) {
        self.poll = link::responder(cx)
            .map(|responder| cx.observe(&responder, |this, _, cx| this.poll_hub(cx)));
    }

    pub(crate) fn stop_polling(&mut self) {
        self.poll = None;
        self.hub_changes = None;
    }

    /// Applies the snapshot when its version changed since the last one.
    fn pull_snapshot(&mut self, cx: &mut Context<Self>) {
        if self.syncing {
            return;
        }
        self.syncing = true;
        for _ in 0..MAX_PULLS {
            let Some(hub) = self.hub.clone() else {
                break;
            };
            let version = hub.version();
            if self.seen == Some(version) {
                break;
            }
            self.seen = Some(version);
            let snapshot = hub.snapshot();
            if let Some(identity) = snapshot.identity() {
                self.initiatives
                    .update(cx, |catalog, cx| catalog.observe_identity(identity, cx));
            }
            if snapshot.scope_epoch() != self.scope_epoch {
                // The update stream closed and NETT's capability list was
                // read again; authority may have changed with it.
                self.scope_epoch = snapshot.scope_epoch();
                self.initiatives
                    .update(cx, |catalog, cx| catalog.refresh(cx));
            }
            self.follow_editor(&snapshot, cx);
            self.follow_issue(&snapshot, cx);
            self.follow_media(&snapshot, cx);
            self.follow_activity(&snapshot);
            self.apply_view(sync::view(&snapshot), cx);
        }
        self.syncing = false;
    }

    /// The board's state follows one snapshot.
    pub(crate) fn apply_view(&mut self, view: HubView, cx: &mut Context<Self>) {
        self.profile_label = Some(view.profile_name);
        self.live = view.live;
        self.set_access(view.access, cx);
        let previous = self.project.clone();
        self.set_projects(view.projects, cx);
        if crate::initiative::no_initiative_projects(self) {
            self.show_status(view.status, cx);
            return;
        }
        // The hub names no team for All teams, as the list query does.
        let listed = match &self.project {
            crate::model::ProjectScope::Project(key) => Some(key.as_str()),
            crate::model::ProjectScope::AllProjects => None,
        };
        let previous_list = view.groups.is_some() && view.list_project.as_deref() != listed;
        if self.initiative_id.is_some() && (self.project != previous || previous_list) {
            return;
        }
        if self.listing.follow_list(view.list) {
            self.invalidate_search_result();
        }
        self.listing.follow_request(view.request_revision);
        self.listing.hidden_subissues_count = view.hidden_subissues_count;
        self.listing.my_issues_count = view.my_issues_count;
        match view.groups {
            Some(groups) => self.show_groups(groups, view.status != Status::Loading, cx),
            None => self.listing.content = None,
        }
        if let (Some(content), Some(read_at)) = (self.listing.content.as_mut(), view.read_at) {
            content.read_at = read_at;
        }
        self.show_status(view.status, cx);
        self.check_hidden_matches(cx);
    }

    fn show_status(&mut self, status: Status, cx: &mut Context<Self>) {
        match status {
            Status::Failed(Failure::Network) if self.listing.content.is_some() => {
                return self.show_failure(Failure::Offline, cx);
            }
            Status::Failed(failure) => return self.show_failure(failure, cx),
            Status::Loading => self.listing.start_read(),
            Status::Idle => self.listing.status = Status::Idle,
        }
        self.sync_lines(cx);
    }

    /// Connection changes invalidate the visible scope.
    pub(crate) fn set_access(&mut self, access: Access, cx: &mut Context<Self>) {
        if self.access == access {
            return;
        }
        self.cancel_search_fallback();
        self.filters.clear_status();
        if matches!(access, Access::Failed(Failure::Edge | Failure::Forbidden)) {
            self.editor = None;
            self.project_move = None;
            self.menu = None;
            self.close_detail(cx);
            self.media.clear();
            self.activity.drop_content();
        }
        self.access = access;
        self.search_pages.clear();
        self.listing.clear();
        self.read_visible(cx);
        self.sync_lines(cx);
    }

    /// Read first pages for open groups, and for folded groups in the nested
    /// view since they can contain children of an open group’s parents.
    pub(crate) fn show_groups(
        &mut self,
        mut groups: Vec<IssueGroup>,
        ask: bool,
        cx: &mut Context<Self>,
    ) {
        crate::listing::prepare_search_groups(&mut groups, &self.query);
        self.listing.settle_unread(&mut groups);
        let mut waiting = Vec::new();
        let nested = self.sub_issues == crate::model::SubIssues::Nested;
        for group in &mut groups {
            if ask && group.more == More::Unread && (nested || !self.group_folded(group)) {
                group.more = More::Loading;
                waiting.push(group.key.clone());
            }
        }
        let next = (ask && waiting.is_empty())
            .then(|| self.next_search_page(&groups))
            .flatten();
        self.listing.loaded(groups, Instant::now());
        for group in waiting {
            self.listing.asked_first_page(&group);
            self.request(Request::Group { group }, cx);
        }
        if let Some(group) = next {
            self.read_more(&group, cx);
        } else {
            self.sync_lines(cx);
        }
    }

    pub(crate) fn show_failure(&mut self, failure: Failure, cx: &mut Context<Self>) {
        self.listing.failed(failure);
        self.sync_lines(cx);
    }
}
