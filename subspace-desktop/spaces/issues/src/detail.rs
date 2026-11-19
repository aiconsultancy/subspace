//! The read-only issue page follows the hub's single open-issue snapshot.
//! The list owns its selection and scroll independently of this page.

mod markdown;
pub(crate) mod timeline;

use std::{collections::HashMap, time::Instant};

use gpui_kit::{AppContext as _, Context, ScrollHandle, SharedString, Window};
use subspace_api_client::hub::{HubSnapshot, IrcSnapshot, IssueSnapshot, IssueStatus, ReadError};
use subspace_framework::contracts::issues::IssueScope;

use crate::Issues;
use crate::listing::updated;
use crate::requests::Request;

fn issue_link(url: &str, snapshot: &IrcSnapshot) -> Option<String> {
    IssueScope::new(
        snapshot.profile_id(),
        snapshot.profile_name(),
        snapshot.api_origin(),
    )
    .with_project_keys(snapshot.project_keys().iter().cloned())
    .navigation_identifier(url)
}

#[derive(Default)]
pub(crate) struct IssueView {
    pub(crate) snapshot: Option<IssueSnapshot>,
    pub(crate) description: Option<SharedString>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) timeline: timeline::TimelineView,
    pub(crate) timeline_focus: Option<timeline::TimelineFocus>,
    pub(crate) comment_bodies: HashMap<String, SharedString>,
    pub(crate) tracker_help:
        Option<gpui_kit::Entity<subspace_framework::dashboard::anchored::Anchor>>,
    pub(crate) workflow_width: f32,
    /// The open issue's end-state change, from the write until the reload
    /// that shows its outcome.
    pub(crate) state_change: Option<StateChange>,
}

/// An end-state change in flight. `reload_after` is unset while the write is
/// on its way; once it returns, it holds the read time of the issue as it was,
/// and the change settles when a newer read, or a failed one, arrives. After a
/// saved write, that read must also carry `written_version` or later, so a
/// read that started before the write cannot settle it.
pub(crate) struct StateChange {
    pub(crate) issue: String,
    pub(crate) reload_after: Option<Option<Instant>>,
    pub(crate) written_version: Option<i64>,
}

impl Issues {
    pub(crate) fn copy_issue_identifier(&self, cx: &mut Context<Self>) {
        if let Some(issue) = &self.issue_view.snapshot {
            subspace_framework::dashboard::links::copy(issue.identifier().to_owned(), cx);
        }
    }
    /// Moves keyboard focus back to the list, such as from the search field.
    pub(crate) fn focus_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.is_none() {
            self.focus.focus(window, cx);
        }
        subspace_framework::dashboard::list::update(self, cx, |_| {});
    }

    pub(crate) fn status_line(&self, now: Instant) -> String {
        if let Some(issue) = &self.issue_view.snapshot {
            return status_line(issue, self.profile_name(), now);
        }
        if self.activity.visible && self.is_ready() {
            return self.activity.status(self.profile_name(), now);
        }
        crate::scope::status_line(self.profile_name(), &self.access, &self.listing, now)
    }

    pub(crate) fn inline_navigation_blocked(&self, cx: &mut Context<Self>) -> bool {
        let blocked = self.inline_field_active();
        if blocked {
            cx.emit(subspace_framework::dashboard::DashboardEvent::Toast(
                subspace_framework::dashboard::Toast::info(
                    "Save or cancel the current field before leaving this issue.",
                ),
            ));
        }
        blocked
    }

    pub(crate) fn open_row(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(identifier) = self.issue(key).map(|item| item.fields.identifier.clone()) {
            self.open_detail(identifier, cx);
        }
    }

    pub(crate) fn open_detail(&mut self, identifier: String, cx: &mut Context<Self>) {
        if self.issue_view.snapshot.is_some() && self.inline_navigation_blocked(cx) {
            return;
        }
        if self.hub.is_none() {
            return;
        }
        // From another issue the first one's origin stays.
        if self.issue_view.snapshot.is_none() {
            self.back.opened_from_list();
        }
        self.decision = None;
        self.issue_view = IssueView::default();
        self.request(Request::OpenIssue { identifier }, cx);
    }

    pub(crate) fn open_issue_link(&mut self, url: &str, cx: &mut Context<Self>) -> bool {
        self.poll_hub(cx);
        let Some(snapshot) = self.hub.as_ref().map(|hub| hub.irc_snapshot()) else {
            return false;
        };
        let Some(identifier) = issue_link(url, &snapshot) else {
            return false;
        };
        if self.editor.is_none() {
            self.open_detail(identifier, cx);
            self.back.opened_by_link();
        }
        true
    }

    pub(crate) fn back_to_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inline_navigation_blocked(cx) {
            return;
        }
        self.close_detail(cx);
        self.focus_list(window, cx);
    }

    pub(crate) fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.back.closed();
        self.authoring.clear();
        self.media.close_issue();
        self.decision = None;
        let was_open = self.issue_view.snapshot.is_some();
        self.issue_view = IssueView::default();
        if let Some(hub) = &self.hub {
            hub.close_issue();
        }
        if was_open {
            cx.notify();
        }
    }

    pub(crate) fn reload_detail(&mut self, cx: &mut Context<Self>) {
        self.request(Request::ReloadIssue, cx);
    }

    pub(crate) fn more_subissues(&mut self, cx: &mut Context<Self>) {
        self.request(Request::MoreSubissues, cx);
    }

    /// Step between issues while skipping the list's group and paging rows.
    /// The list selection follows the issue so Back reveals that row.
    pub(crate) fn step_issue(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.inline_navigation_blocked(cx) {
            return;
        }
        if self.activity.visible {
            self.step_activity(forward, cx);
            return;
        }
        let keys = self.list.visible_keys();
        let Some(key) = adjacent(&keys, self.list.selected(), forward) else {
            return;
        };
        self.list.select(key.clone());
        self.open_row(key, cx);
        self.load_more_near(Some(key.clone()), cx);
    }

    pub(crate) fn follow_issue(&mut self, snapshot: &HubSnapshot, cx: &mut Context<Self>) {
        let Some(issue) = snapshot
            .issue()
            .filter(|issue| issue.profile_id() == self.profile)
        else {
            self.decision = None;
            self.issue_view = IssueView::default();
            self.follow_comments(snapshot, cx);
            return;
        };
        self.issue_view.follow(issue, cx);
        crate::decision::follow(&mut self.decision, issue);
        self.settle_state_change();
        self.start_timeline(issue, cx);
        self.follow_comments(snapshot, cx);
    }
}

impl IssueView {
    fn follow(&mut self, issue: &IssueSnapshot, cx: &mut Context<Issues>) {
        let previous = self.snapshot.as_ref();
        if issue.detail().is_none() {
            self.description = None;
        } else {
            self.description = issue
                .detail()
                .and_then(|detail| markdown::description(detail, issue.files().items()));
        }
        if previous.is_some_and(|old| old.identifier() != issue.identifier()) {
            self.timeline = Default::default();
        }
        self.timeline_focus
            .get_or_insert_with(|| timeline::TimelineFocus::new(&self.scroll, cx));
        self.tracker_help
            .get_or_insert_with(|| cx.new(subspace_framework::dashboard::anchored::Anchor::new));
        let old_page = previous.and_then(|old| old.timeline().page());
        let posted_changed = previous.is_none_or(|old| {
            old.timeline().projected().iter().map(|c| &c.id).ne(issue
                .timeline()
                .projected()
                .iter()
                .map(|c| &c.id))
        });
        match issue.timeline().page() {
            Some(page) if posted_changed || old_page.is_none_or(|old| !std::ptr::eq(old, page)) => {
                self.comment_bodies = markdown::comments(&page.items, issue.timeline().projected());
            }
            None => self.comment_bodies = markdown::comments(&[], issue.timeline().projected()),
            _ => {}
        }
        self.snapshot = Some(issue.clone());
    }
}

fn adjacent<'a>(
    keys: &'a [SharedString],
    selected: Option<&SharedString>,
    forward: bool,
) -> Option<&'a SharedString> {
    let index = keys.iter().position(|key| Some(key) == selected)?;
    let is_issue =
        |key: &&SharedString| matches!(crate::lines::line(key), Some(crate::lines::Line::Issue(_)));
    if forward {
        keys[index + 1..].iter().find(is_issue)
    } else {
        keys[..index].iter().rev().find(is_issue)
    }
}

pub(crate) fn status_line(issue: &IssueSnapshot, profile: &str, now: Instant) -> String {
    match issue.status() {
        IssueStatus::Loading => format!("{profile} · Loading {}", issue.identifier()),
        IssueStatus::Loaded => issue.updated_at().map_or_else(
            || profile.to_owned(),
            |at| format!("{profile} · {}", updated(at, now)),
        ),
        IssueStatus::Error(ReadError::Network) => format!("{profile} · Offline"),
        _ => profile.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::adjacent;
    use gpui_kit::SharedString;

    #[test]
    fn api_navigation_is_bound_to_the_current_api_and_profile() {
        let fixture = crate::tests::offline::offline();
        let (hub, _) = crate::tests::offline::fixture_hub(&fixture);
        crate::tests::offline::wait_for(|| hub.snapshot().me().is_some());
        let snapshot = hub.irc_snapshot();
        let address = format!(
            "{}/v1/issues/SUB-1?profile=subspace-api",
            snapshot.api_origin()
        );
        assert_eq!(
            super::issue_link(&address, &snapshot).as_deref(),
            Some("SUB-1")
        );
        assert_eq!(
            super::issue_link(&address.replace("subspace-api", "other"), &snapshot),
            None
        );
        assert_eq!(
            super::issue_link("https://legacy.example.invalid/issues/SUB-1", &snapshot),
            None
        );
        hub.stop();
    }

    #[test]
    fn detail_steps_in_list_order_and_skips_non_issue_rows() {
        let keys: Vec<SharedString> = [
            "group:started",
            "issue:a",
            "more:started",
            "group:todo",
            "issue:b",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert_eq!(adjacent(&keys, Some(&keys[1]), true), Some(&keys[4]));
        assert_eq!(adjacent(&keys, Some(&keys[4]), false), Some(&keys[1]));
        assert_eq!(adjacent(&keys, Some(&keys[4]), true), None);
        assert_eq!(adjacent(&keys, Some(&keys[1]), false), None);
    }
}
