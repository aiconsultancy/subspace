//! How the Space fills the shared workspace shell: the navigator with its
//! views and repositories, the pane's tab strip and toolbar around the shared
//! list or the opened item, and the docks it reports for the status bar's
//! two switches.

use gpui_kit::{
    AnyElement, App, ClickEvent, Context, IntoElement as _, ParentElement as _, Styled as _,
    Window, div,
};
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::list;
use subspace_framework::dashboard::tabs::{self, TabSpec};
use subspace_framework::dashboard::workspace::{
    NavRow, Navigator, Pane, SpaceDock, SpaceDocks, Toolbar, nav_section, pane_head,
};
use subspace_framework::dashboard::{Dashboard as _, prefs, truncated};
use subspace_framework::lucide::IconName;

use crate::CopilotAgents;
use crate::keys::{INSPECTOR_KEY, NAVIGATOR_KEY, Picking};
use crate::model::Kind;
use crate::navigator::{self, Destination};
use crate::peek;
use crate::ui::{dock_shown, dock_value};

/// The navigator footer's account line.
const ACCOUNT_ID: &str = "github-copilot-agents-account";
/// The opened item's Back, which shows the list again.
pub(crate) const BACK_ID: &str = "github-copilot-agents-back";
/// The pane's toolbar, the list's or the opened item's.
const TOOLBAR_ID: &str = "github-copilot-agents-toolbar";
/// The opened tab's name once its row has left the list.
const GONE_TAB: &str = "Not listed";

// qual:allow(coupling, oi) reason: "One file per concern of the Copilot Agents dashboard, as its list, detail and filter files already are; this one owns the workspace shell around the list."
impl CopilotAgents {
    /// The navigator: the views, the repositories of the loaded rows, and
    /// the account the tasks belong to.
    pub(crate) fn navigator(&self, cx: &Context<Self>) -> Navigator {
        let views = navigator::views(&self.all_rows, &self.filters);
        let repositories = navigator::repositories(&self.all_rows, &self.filters);
        let heading = (!repositories.is_empty()).then(|| nav_section("Repositories", cx));
        let account = truncated::text(ACCOUNT_ID, self.account().footer()).flex_1();
        Navigator::new(Self::TITLE)
            .child(nav_section("Views", cx))
            .children(views.into_iter().map(|view| view_row(view, cx)))
            .children(heading)
            .children(repositories.into_iter().map(|repo| repo_row(repo, cx)))
            .footer(account)
    }

    /// The pane: the list's tab and, beside it, the opened item's, then
    /// the toolbar, above the shared list or the opened item's page.
    pub(crate) fn pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tabs = self.open_tabs(cx).into_any_element();
        match self.item.showing().map(str::to_owned) {
            Some(key) => {
                let toolbar = self.item_toolbar(cx).into_any_element();
                Pane::new("github-copilot-agents-pane")
                    .tabs(tabs)
                    .toolbar(toolbar)
                    .child(self.item_page(&key, cx))
                    .into_any_element()
            }
            None => {
                let toolbar = self.pane_toolbar(cx).into_any_element();
                let head = pane_head(Some(tabs), Some(toolbar), cx);
                list::render_under(self, head, window, cx).into_any_element()
            }
        }
    }

    /// The list's tab, named after the view on show, then the opened
    /// item's, which closes. The opened tab stays while its row has left
    /// the list, so the page that says so can still be closed.
    fn open_tabs(&self, cx: &mut Context<Self>) -> tabs::TabBar {
        let view = self.filters.kind;
        let bar = tabs::pane_tab_bar("github-copilot-agents-tabs")
            .selected_index(self.item.tab_index())
            .child(tabs::tab(TabSpec::new(
                navigator::view_label(view),
                view_icon(view),
            )))
            .on_click(cx.listener(|this, index: &usize, _, cx| this.show_tab(*index, cx)));
        let Some(key) = self.item.key.as_deref() else {
            return bar;
        };
        let tab = match self.row(key) {
            Some(row) => tabs::tab(TabSpec::new(
                peek::tab_title(row),
                view_icon(Some(row.kind)),
            ))
            .aria_label(format!("{}: {}", peek::name(row), row.title)),
            None => tabs::tab(TabSpec::new(GONE_TAB, view_icon(None)))
                .aria_label("Opened task: no longer listed"),
        };
        bar.child(tab.on_close(cx.listener(|this, _: &ClickEvent, _, cx| this.close_item(cx))))
    }

    /// The list's toolbar: the filters. The actions on the selection sit in
    /// the inspector.
    fn pane_toolbar(&mut self, cx: &mut Context<Self>) -> Toolbar {
        let filters = self.toolbar(cx);
        Toolbar::new(TOOLBAR_ID).leading(div().min_w_0().max_w_full().child(filters))
    }

    /// The opened item's toolbar: Back to the list, and its page on GitHub.
    fn item_toolbar(&self, cx: &mut Context<Self>) -> Toolbar {
        let back = icon_button(
            BACK_ID,
            Intent::Quiet,
            IconName::ArrowLeft,
            "Back to the list",
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.show_list(cx)));
        let bar = Toolbar::new(TOOLBAR_ID).leading(back);
        match self.browser_button(cx) {
            Some(browser) => bar.trailing(browser),
            None => bar,
        }
    }

    /// Both docks, as they show, for the status bar's switches.
    pub(crate) fn space_docks(&self) -> SpaceDocks {
        SpaceDocks::both(self.navigator_open, self.inspector_open)
    }

    /// A dock's status bar switch was used.
    pub(crate) fn toggle_space_dock(&mut self, dock: SpaceDock, cx: &mut Context<Self>) {
        match dock {
            SpaceDock::Navigator => self.toggle_navigator(cx),
            SpaceDock::Inspector => self.toggle_inspector(cx),
        }
    }

    /// Shows or hides the navigator, and remembers the choice.
    pub(crate) fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        self.navigator_open = !self.navigator_open;
        prefs::set(cx, NAVIGATOR_KEY, dock_value(self.navigator_open));
        cx.notify();
    }

    /// Shows or hides the inspector, and remembers the choice. Shown, it
    /// loads the selection.
    pub(crate) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        let show = !self.inspector_open;
        self.set_inspector(show, cx);
        self.sync_detail(cx);
        cx.notify();
    }

    /// Records whether the inspector shows.
    pub(crate) fn set_inspector(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.inspector_open = shown;
        prefs::set(cx, INSPECTOR_KEY, dock_value(shown));
    }
}

/// Whether the navigator and the inspector show, as last stored. Both
/// show until hidden.
pub(crate) fn stored_docks(cx: &App) -> (bool, bool) {
    (
        dock_shown(prefs::get(cx, NAVIGATOR_KEY).as_deref()),
        dock_shown(prefs::get(cx, INSPECTOR_KEY).as_deref()),
    )
}

/// One view as a navigator row. A click makes it the kind filter.
fn view_row(view: Destination<Option<Kind>>, cx: &Context<CopilotAgents>) -> NavRow {
    let value = navigator::view_value(view.value);
    NavRow::new(
        navigator::view_key(view.value),
        navigator::view_label(view.value),
    )
    .icon(view_icon(view.value))
    .count(view.count.to_string())
    .selected(view.selected)
    .quiet(view.count == 0)
    .on_click(cx.listener(move |this, _, _, cx| {
        this.pick_filter(Picking::Kind, value, cx);
        this.show_list(cx);
    }))
}

/// One repository as a navigator row. A click makes it the repository
/// filter, and a click on the chosen one clears that filter.
fn repo_row(repo: Destination<String>, cx: &Context<CopilotAgents>) -> NavRow {
    let choice = navigator::repo_choice(&repo.value, repo.selected).to_owned();
    NavRow::new(navigator::repo_key(&repo.value), repo.value)
        .icon(IconName::GitBranch)
        .count(repo.count.to_string())
        .selected(repo.selected)
        .quiet(repo.count == 0)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.pick_filter(Picking::Repo, &choice, cx);
            this.show_list(cx);
        }))
}

/// The glyph a view carries in the navigator and on the pane's tab.
fn view_icon(view: Option<Kind>) -> IconName {
    match view {
        None => IconName::Layers,
        Some(Kind::Cloud) => IconName::Cloud,
        Some(Kind::Local) => IconName::MessageSquare,
    }
}

#[cfg(test)]
mod tests;
