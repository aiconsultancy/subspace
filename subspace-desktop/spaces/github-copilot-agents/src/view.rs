//! How the Copilot Agents dashboard draws: the shared list, the detail panel it
//! fills, and the actions its view answers.

use gpui_kit::{
    AnyElement, App, Context, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Window,
};
use subspace_framework::dashboard::list::{self, Detail, ListDashboard, ListState, Row};
use subspace_framework::dashboard::row_menu::RowMenu;

use crate::keys::{ClearFilters, GroupMenu, KindMenu, Picking, RepoMenu, RetrySessions};
use crate::model::Kind;
use crate::{CopilotAgents, logs, panels};

impl ListDashboard for CopilotAgents {
    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    fn detail(&self, key: &str) -> Option<Detail> {
        let row = self.row(key)?;
        if row.kind == Kind::Local {
            return Some(panels::local_detail(row));
        }
        let loaded = self
            .detail
            .as_ref()
            .filter(|_| self.detail_key.as_deref() == Some(key));
        let mut sections = match loaded {
            Some(detail) => panels::session_sections(detail),
            None if self.detail_error.is_some() => Vec::new(),
            None => vec![panels::note_section(&row.state)],
        };
        sections.extend(
            self.log
                .as_ref()
                .and_then(|open| logs::log_section(open, key)),
        );
        Some(panels::cloud_detail(row, sections))
    }

    fn empty_message(&self) -> (SharedString, SharedString) {
        match self.filters.is_empty() {
            true => (
                "No agent tasks or chat sessions".into(),
                "Start a task with Fix with Copilot, or a local conversation in Chat.".into(),
            ),
            false => (
                "Nothing matches the filters".into(),
                "Press 0 to clear the kind, state and repository filters.".into(),
            ),
        }
    }

    fn on_open(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.load_detail(&key, cx);
    }

    fn panel_extra(&self, key: &str, cx: &App) -> Option<AnyElement> {
        let message = self
            .detail_error
            .as_ref()
            .filter(|_| self.detail_key.as_deref() == Some(key))?;
        Some(
            subspace_framework::dashboard::feedback::error_banner(
                &subspace_framework::dashboard::Tokens::current(cx),
                "github-copilot-agents-banner",
                "Cannot load sessions",
                message.clone(),
                |_, window, cx| window.dispatch_action(Box::new(RetrySessions), cx),
            )
            .into_any_element(),
        )
    }

    /// The row's GitHub page, so the menu can open and copy it.
    fn row_menu(&self, row: &Row) -> RowMenu {
        let facts = RowMenu::from_row(row);
        match self.row(row.key.as_ref()).and_then(crate::browser::link) {
            Some(link) => facts.link("GitHub", &link.url),
            None => facts,
        }
    }
}

impl Focusable for CopilotAgents {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CopilotAgents {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = self.menu_anchors.view();
        list::render(self, window, cx)
            .on_action(cx.listener(Self::open_session))
            .on_action(cx.listener(Self::open_pull_request))
            .on_action(cx.listener(Self::copy_link))
            .on_action(cx.listener(Self::toggle_monitor))
            .on_action(cx.listener(Self::open_subject))
            .on_action(cx.listener(Self::open_in_chat))
            .on_action(cx.listener(Self::show_log))
            .on_action(cx.listener(Self::retry_sessions))
            .on_action(cx.listener(|this, _: &KindMenu, window, cx| {
                this.open_filter_menu(Picking::Kind, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GroupMenu, window, cx| {
                this.open_filter_menu(Picking::Group, window, cx);
            }))
            .on_action(cx.listener(|this, _: &RepoMenu, window, cx| {
                this.open_filter_menu(Picking::Repo, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClearFilters, _, cx| this.clear_filters(cx)))
            .children(menu)
    }
}
