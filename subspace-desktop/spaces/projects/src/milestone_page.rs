//! The project milestone page and its link back from Issues.

use std::collections::HashMap;

use gpui_kit::{AppContext as _, Context, ScrollHandle, Task, point, px};
use subspace_api_client::model::IssueListItem;
use subspace_api_client::projects::Milestone;
use subspace_api_client::{Client, Error, signed_in_generation};
use subspace_framework::contracts::issues::CurrentScope;
use subspace_framework::dashboard::contracts::{self, BoardContracts, Declaration, MessageKey};
use subspace_framework::dashboard::{
    BoardNavigation, Dashboard, DashboardEvent, ReturnLocation, Toast,
};

use crate::{Projects, Tab};

pub(crate) const CONTRACTS: BoardContracts =
    BoardContracts::new(&[Declaration::requests::<CurrentScope>()]);

#[derive(Default)]
pub(crate) struct Pages {
    pub(crate) preview: Option<String>,
    pub(crate) detail: Option<Detail>,
    pub(crate) overview_scroll: ScrollHandle,
    pub(crate) list_scroll: ScrollHandle,
    pub(crate) detail_scroll: ScrollHandle,
    pub(crate) issues: HashMap<String, Result<Vec<IssueListItem>, Error>>,
    pending: HashMap<String, Task<()>>,
}

pub(crate) struct Detail {
    pub(crate) milestone: String,
    pub(crate) from: Tab,
}

impl Pages {
    pub(crate) fn scroll(&self, tab: Tab) -> &ScrollHandle {
        match (self.detail.is_some(), tab) {
            (true, _) => &self.detail_scroll,
            (_, Tab::Overview) => &self.overview_scroll,
            (_, Tab::Milestones) => &self.list_scroll,
        }
    }
}

// qual:allow(coupling, oi) reason: "Milestone navigation and reads share the selected project, scroll positions and account lease."
impl Projects {
    pub(crate) fn milestone(&self, id: &str) -> Option<&Milestone> {
        let record = self.record.as_ref()?.as_ref().ok()?;
        if !self.in_scope(&record.project) {
            return None;
        }
        record
            .milestones
            .as_ref()
            .ok()?
            .iter()
            .find(|row| row.id == id)
    }

    pub(crate) fn preview_milestone(&mut self, id: &str, hovered: bool, cx: &mut Context<Self>) {
        if hovered && self.milestone(id).is_some() {
            self.milestone_pages.preview = Some(id.to_owned());
            self.fetch_issues(id, cx);
        } else if self.milestone_pages.preview.as_deref() == Some(id) {
            self.milestone_pages.preview = None;
        }
        cx.notify();
    }

    pub(crate) fn open_milestone(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.milestone(id).is_some() && self.editor.is_none() {
            self.show_milestone(id, self.tab, cx);
        }
    }

    fn show_milestone(&mut self, id: &str, from: Tab, cx: &mut Context<Self>) {
        self.milestone_pages.preview = None;
        self.milestone_pages.detail = Some(Detail {
            milestone: id.into(),
            from,
        });
        self.milestone_pages.detail_scroll = ScrollHandle::new();
        self.tab = Tab::Milestones;
        self.retry_milestone_issues(id, cx);
        cx.notify();
    }

    pub(crate) fn close_milestone(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = self.milestone_pages.detail.take() {
            self.tab = detail.from;
            cx.notify();
        }
    }

    pub(crate) fn open_linked_issue(&mut self, issue: &str, cx: &mut Context<Self>) {
        if self.account != signed_in_generation() {
            return;
        }
        let origin = self.milestone_place();
        let scope = contracts::request(Self::ID, &CurrentScope, cx)
            .ok()
            .flatten();
        if let (Some(scope), Some(origin)) = (scope, origin)
            && let Some(url) = scope.navigation_url(&format!(
                "{}/v1/issues/{issue}",
                scope.api_origin.trim_end_matches('/')
            ))
        {
            cx.emit(DashboardEvent::Navigate(BoardNavigation {
                responder: MessageKey::request::<CurrentScope>(),
                url: url.into(),
                origin: Some(origin),
            }));
        } else {
            cx.emit(DashboardEvent::Toast(Toast::info(format!(
                "Couldn’t open {issue}. Check that Issues is enabled and signed in."
            ))));
        }
    }

    pub(crate) fn milestone_place(&self) -> Option<ReturnLocation> {
        let detail = self.milestone_pages.detail.as_ref()?;
        let milestone = self.milestone(&detail.milestone)?;
        let previous = match detail.from {
            Tab::Overview => "overview",
            Tab::Milestones => "milestones",
        };
        let offset = f32::from(self.milestone_pages.detail_scroll.offset().y);
        let account = self
            .account
            .map_or_else(|| "-".to_owned(), |account| account.to_string());
        Some(ReturnLocation {
            label: milestone.name.clone().into(),
            key: format!(
                "milestone:{account}:{}:{}:{previous}:{offset}",
                self.selected.as_ref()?,
                milestone.id
            )
            .into(),
        })
    }

    pub(crate) fn return_to_milestone(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        self.follow_account(cx);
        let Some((account, project, milestone, from, offset)) = return_parts(key) else {
            return false;
        };
        if self.account != account
            || self.editor.is_some()
            || self.write_task.is_some()
            || !self.visible_projects().any(|row| row.id == project)
        {
            return false;
        }
        self.select(project, cx);
        self.show_milestone(milestone, from, cx);
        self.milestone_pages
            .detail_scroll
            .set_offset(point(Default::default(), px(offset)));
        true
    }

    fn fetch_issues(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.milestone_pages.pending.contains_key(id) {
            return;
        }
        let id = id.to_owned();
        let requested = id.clone();
        let account = self.account;
        let project = self.selected.clone();
        let read = cx.background_spawn(async move { Client::new()?.milestone_issues(&requested) });
        let task = cx.spawn({
            let id = id.clone();
            async move |this, cx| {
                let result = read.await;
                _ = this.update(cx, |this, cx| {
                    if signed_in_generation() == account && this.selected == project {
                        this.milestone_pages.pending.remove(&id);
                        this.milestone_pages.issues.insert(id, result);
                        cx.notify();
                    }
                });
            }
        });
        self.milestone_pages.pending.insert(id, task);
    }

    pub(crate) fn retry_milestone_issues(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .milestone_pages
            .issues
            .get(id)
            .is_some_and(Result::is_err)
        {
            self.milestone_pages.issues.remove(id);
        }
        self.fetch_issues(id, cx);
        cx.notify();
    }
}

fn return_parts(key: &str) -> Option<(Option<u64>, &str, &str, Tab, f32)> {
    let [account, project, milestone, from, offset] = key
        .strip_prefix("milestone:")?
        .split(':')
        .collect::<Vec<_>>()
        .try_into()
        .ok()?;
    let tab = match from {
        "overview" => Tab::Overview,
        "milestones" => Tab::Milestones,
        _ => return None,
    };
    let offset: f32 = offset.parse().ok()?;
    let account = if account == "-" {
        None
    } else {
        Some(account.parse().ok()?)
    };
    offset
        .is_finite()
        .then_some((account, project, milestone, tab, offset))
}

#[cfg(test)]
mod tests {
    use super::return_parts;
    use crate::Tab;

    #[test]
    fn milestone_return_key_keeps_the_origin_tab_and_scroll_position() {
        let (account, project, milestone, origin, offset) =
            return_parts("milestone:4:project-1:milestone-2:overview:-235").unwrap();
        assert_eq!(
            (account, project, milestone, offset),
            (Some(4), "project-1", "milestone-2", -235.)
        );
        assert!(origin == Tab::Overview);
        assert!(return_parts("milestone:4:project-1:milestone-2:unknown:0").is_none());
        assert!(return_parts("milestone:4:project-1:milestone-2:overview:NaN").is_none());
        assert!(return_parts("milestone:-:project-1:milestone-2:milestones:0").is_some());
    }
}
