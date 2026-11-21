//! How the Copilot Agents dashboard draws: the workspace shell around the
//! shared list or the opened item, the inspector beside them, and the
//! actions its view answers.

use chrono::Utc;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, Div, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    KeyContext, ParentElement as _, Render, SharedString, Styled as _, Window, div,
};
use subspace_framework::dashboard::list::{Detail, Group, ListDashboard, ListState, Row};
use subspace_framework::dashboard::row_menu::RowMenu;
use subspace_framework::dashboard::workspace::Workspace;
use subspace_framework::dashboard::{CloseDetail, DASHBOARD_CONTEXT, Dashboard as _, Tokens};

use crate::keys::{ClearFilters, GroupMenu, KindMenu, Picking, RepoMenu};
use crate::{CopilotAgents, lanes, peek};

impl ListDashboard for CopilotAgents {
    fn list(&self) -> &ListState {
        &self.list
    }

    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }

    /// The opened item's page with its log, as the old detail panel held
    /// it. The shared panel itself never draws: an opened row is a tab.
    fn detail(&self, key: &str) -> Option<Detail> {
        let mut detail = self.item_detail(key)?;
        detail.sections.extend(self.item_log(key));
        Some(detail)
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

    /// Enter, a double click, a search result or the menu's Open: the row
    /// opens as a tab beside the list, and the shared panel stays shut.
    fn on_open(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.list.close_detail();
        self.open_item(&key, cx);
    }

    /// The selection loads in the inspector.
    fn on_selection_changed(&mut self, _selected: Option<SharedString>, cx: &mut Context<Self>) {
        self.sync_detail(cx);
    }

    /// A click only selects a row; a double click or Enter opens it. A
    /// group header still folds on a click.
    fn activates_on_click(&self, _row: &Row) -> bool {
        false
    }

    fn comfortable_rows(&self) -> bool {
        true
    }

    fn group_count_after_name(&self) -> bool {
        true
    }

    fn row_leading(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        lanes::leading(self.row(&row.key)?, cx)
    }

    fn row_trailing(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        lanes::trailing(self.row(&row.key)?, cx)
    }

    fn group_leading(&self, group: &Group, cx: &App) -> Option<AnyElement> {
        lanes::group_mark(group, cx)
    }

    fn row_accessible_label(&self, row: &Row, _cx: &App) -> SharedString {
        match self.row(&row.key) {
            Some(agent) => peek::row_label(agent, Utc::now()).into(),
            None => row.title.clone(),
        }
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
        let tokens = Tokens::current(cx);
        let menu = self.menu_anchors.view();
        let pane = self.pane(window, cx);
        let workspace = Workspace::new("github-copilot-agents-shell").pane(pane);
        let workspace = match self.navigator_open {
            true => workspace.navigator(self.navigator(cx)),
            false => workspace,
        };
        let workspace = match self.inspector_open {
            true => workspace.inspector(self.inspector(cx)),
            false => workspace,
        };
        with_actions(root(self, cx), cx)
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .child(div().flex_1().min_h_0().child(workspace))
            .children(menu)
    }
}

/// The Space's root: its key context and its focus, so its own keys answer
/// wherever in the workspace the pointer last went.
fn root(space: &CopilotAgents, cx: &Context<CopilotAgents>) -> Div {
    // An element declares context names; `view_keys` is a binding
    // predicate and must not be parsed as one.
    let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
    context.add(DASHBOARD_CONTEXT);
    context.add(CopilotAgents::ID);
    v_flex()
        .size_full()
        .relative()
        .key_context(context)
        .track_focus(&space.focus)
}

/// The Space's own actions, answered from its root. The shared list answers
/// its own keys from inside the pane.
fn with_actions(root: Div, cx: &mut Context<CopilotAgents>) -> Div {
    root.on_action(cx.listener(CopilotAgents::open_session))
        .on_action(cx.listener(CopilotAgents::open_pull_request))
        .on_action(cx.listener(CopilotAgents::copy_link))
        .on_action(cx.listener(CopilotAgents::toggle_monitor))
        .on_action(cx.listener(CopilotAgents::open_subject))
        .on_action(cx.listener(CopilotAgents::open_in_chat))
        .on_action(cx.listener(CopilotAgents::show_log))
        .on_action(cx.listener(CopilotAgents::retry_sessions))
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
        .on_action(cx.listener(|this, _: &CloseDetail, _, cx| this.show_list(cx)))
}
