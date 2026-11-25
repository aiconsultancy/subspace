//! The Space's workspace layout: the navigator with its views, the pane
//! with the list (or an open pull request's page, `page`), the inspector
//! (`inspector`), and the words of the status line (`words`), which are
//! pure, so the tests run them without a window.
//!
//! Under the views, the navigator lists the view's repositories, which
//! filter the list to one repository (`repo_filter`).

use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::client_github::model::{SectionDef, short_login};
use subspace_framework::dashboard::action::{Intent, icon_button, menu_button};
use subspace_framework::dashboard::list;
use subspace_framework::dashboard::menu::MenuOption;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabBar, TabSpec};
use subspace_framework::dashboard::workspace::{
    NavRow, Navigator, Pane, SpaceDock, SpaceDocks, Toolbar, Workspace, nav_section, text_edge,
};
use subspace_framework::dashboard::{Dashboard as _, Tokens, prefs};

use crate::model::{section_by_id, sections_of};
use crate::{
    Github, MenuRequest, MenuTarget, SectionStatus, SwitchAccount, favourites, github_section_icon,
};

mod words;

pub(crate) use words::{Load, Signed, ViewCount, counts_line, status_line};

/// Whether the navigator shows: `0` once it is hidden, `1` when it shows
/// again. It shows until it is hidden.
pub(crate) const NAVIGATOR_KEY: &str = "github.navigator";
/// Whether the inspector, the detail panel, shows, stored the same way.
pub(crate) const INSPECTOR_KEY: &str = "github.inspector";
/// The anchor of the view menu, the control that stands in for the
/// navigator's views while the navigator is hidden.
pub(crate) const VIEW_ANCHOR: &str = "gh-view";
/// What a dock stores while it shows, and while it is hidden.
const DOCK_SHOWN: &str = "1";
const DOCK_HIDDEN: &str = "0";

/// Whether a dock shows, from its stored value: it does unless it was
/// hidden.
pub(crate) fn dock_shown(stored: Option<&str>) -> bool {
    stored != Some(DOCK_HIDDEN)
}

/// What a dock stores for being shown or hidden.
pub(crate) fn dock_value(shown: bool) -> &'static str {
    if shown { DOCK_SHOWN } else { DOCK_HIDDEN }
}

/// Whether the dock stored under `key` shows.
pub(crate) fn dock_open(cx: &App, key: &str) -> bool {
    dock_shown(prefs::get(cx, key).as_deref())
}

/// The workspace's text edge as the theme has it now: where the navigator's
/// rows, the toolbar and the list's rows start.
pub(crate) fn current_text_edge(cx: &App) -> gpui_kit::Pixels {
    text_edge(&Tokens::current(cx))
}

/// What the layout reads from the Space.
impl Github {
    /// What the Space knows about the signed-in account.
    pub(crate) fn signed(&self) -> Signed<'_> {
        match self.account_login() {
            _ if self.accounts_error.is_some() => Signed::Unavailable,
            Some(login) if !self.accounts.is_empty() => Signed::In(login),
            _ if self.started => Signed::Checking,
            _ => Signed::Unknown,
        }
    }

    /// The total of view `section` and how its last load went.
    pub(crate) fn view_count(&self, section: &str) -> ViewCount {
        let account = self.account_data();
        let data = account.and_then(|account| account.sections.get(section));
        let total = data.map(|data| data.total);
        let load = match account.and_then(|account| account.status.get(section)) {
            Some(SectionStatus::Failed(_)) => Load::Failed,
            Some(SectionStatus::Loading) => Load::Loading,
            // A view without a list yet is still on its way.
            _ if total.is_none() => Load::Loading,
            _ => Load::Ready,
        };
        ViewCount { total, load }
    }

    /// The current view's name, such as "Review requested".
    pub(crate) fn view_title(&self) -> &'static str {
        match section_by_id(self.section) {
            Some(section) => section.title,
            None => self.section,
        }
    }
}

/// The dock switches and the view menu.
impl Github {
    /// Shows or hides the navigator, and remembers the choice.
    pub(crate) fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        self.navigator_open = !self.navigator_open;
        prefs::set(cx, NAVIGATOR_KEY, dock_value(self.navigator_open));
        cx.notify();
    }

    /// The inspector switch, and the inspector's Close: shows or hides the
    /// inspector, and remembers the choice. An open page stays open.
    pub(crate) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        let show = !self.inspector_open;
        self.set_inspector(show, cx);
        cx.notify();
    }

    /// Records whether the inspector shows.
    pub(crate) fn set_inspector(&mut self, shown: bool, cx: &mut Context<Self>) {
        if self.inspector_open == shown {
            return;
        }
        self.inspector_open = shown;
        prefs::set(cx, INSPECTOR_KEY, dock_value(shown));
    }

    /// Opens the view menu at its toolbar control.
    fn open_view_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut options: Vec<MenuOption> = sections_of(self.kind())
            .map(|section| self.view_option(section))
            .collect();
        options.extend(self.repository_options());
        let request = MenuRequest {
            label: "View",
            placeholder: "Go to a view or repository",
            options,
            target: MenuTarget::View,
        };
        self.open_menu_at(request, None, window, cx);
    }

    /// A view as the view menu lists it: its name, then its count in words.
    fn view_option(&self, section: &SectionDef) -> MenuOption {
        let words = self.view_count(section.id).words();
        let label = format!("{} · {words}", section.title);
        MenuOption::new(section.id, label).checked(section.id == self.section)
    }
}

/// The workspace's elements.
impl Github {
    /// The list view as a workspace: the navigator, unless it is hidden,
    /// the pane, and the inspector, unless it is hidden. The pane shows the
    /// shared list under its tab strip and toolbar, or the page of the open
    /// pull request. The list never draws its own detail panel: a row it
    /// opens is the page.
    pub(crate) fn workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Workspace {
        self.text_edge = current_text_edge(cx);
        let open = self.list.open_row().map(|row| row.key.clone());
        let pane = match open {
            Some(key) => self.page_pane(&key, cx),
            None => {
                let toolbar = self.toolbar(cx);
                let content = list::render(self, window, cx);
                Pane::new("github-pane")
                    .tabs(self.view_tab())
                    .toolbar(toolbar)
                    .child(div().flex_1().min_h_0().child(content))
            }
        };
        let workspace = Workspace::new("github-workspace").pane(pane);
        let workspace = match self.navigator_open {
            true => workspace.navigator(self.navigator(cx)),
            false => workspace,
        };
        match self.inspector_open {
            true => workspace.inspector(self.inspector(cx)),
            false => workspace,
        }
    }

    /// The navigator: the views with their counts, the view's repositories,
    /// then the account in the footer.
    fn navigator(&self, cx: &mut Context<Self>) -> Navigator {
        let mut navigator = Navigator::new("Pull requests")
            .child(nav_section("Views", cx))
            .children(self.view_rows(cx));
        let repositories = self.repository_rows(cx);
        if !repositories.is_empty() {
            navigator = navigator
                .child(nav_section("Repositories", cx))
                .children(repositories);
        }
        navigator.footer(self.account_footer(cx))
    }

    /// One row per view; the current one is selected.
    fn view_rows(&self, cx: &Context<Self>) -> Vec<NavRow> {
        sections_of(self.kind())
            .map(|section| {
                let id = section.id;
                NavRow::new(format!("gh-view:{id}"), section.title)
                    .icon(github_section_icon(id))
                    .count(self.view_count(id).mark())
                    .selected(id == self.section)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.go_section(id, cx);
                    }))
            })
            .collect()
    }

    /// The navigator's footer: the account picker when more than one account
    /// is signed in, else the line that names the account.
    fn account_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.accounts.len() > 1 {
            return self.account_button(cx);
        }
        let tokens = Tokens::current(cx);
        let line = SharedString::from(self.signed().footer_line());
        h_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(
                Icon::from(IconName::User)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(tokens.muted),
            )
            .child(
                div()
                    .id("gh-account-line")
                    .test_support()
                    .aria_label(line.clone())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(line),
            )
            .into_any_element()
    }

    /// The pane's one tab: the list of the current view.
    fn view_tab(&self) -> TabBar {
        let spec = TabSpec::new(self.view_title(), github_section_icon(self.section));
        dashboard_tabs::pane_tab_bar("gh-open-tabs")
            .selected_index(0)
            .child(dashboard_tabs::tab(spec))
    }

    /// The docks the status bar's switches show: both, as they show now,
    /// while the workspace is on show; none while a diff fills the Space.
    pub(crate) fn space_docks(&self) -> SpaceDocks {
        match self.diff.view() {
            Some(_) => SpaceDocks::NONE,
            None => SpaceDocks::both(self.navigator_open, self.inspector_open),
        }
    }

    /// A dock's status bar switch was used.
    pub(crate) fn toggle_space_dock(&mut self, dock: SpaceDock, cx: &mut Context<Self>) {
        match dock {
            SpaceDock::Navigator => self.toggle_navigator(cx),
            SpaceDock::Inspector => self.toggle_inspector(cx),
        }
    }

    /// The pane's toolbar. Leading: the view menu while the navigator is
    /// hidden, Repositories, the repository filter's clear button while one
    /// is set, and the account picker while the navigator is hidden.
    /// Trailing: Refresh. The dock switches are the status bar's.
    fn toolbar(&self, cx: &mut Context<Self>) -> Toolbar {
        let toolbar = Toolbar::new("github-list-toolbar");
        let toolbar = match self.navigator_open {
            true => toolbar,
            false => toolbar.leading(self.view_button(cx)),
        };
        let toolbar = toolbar.leading(self.repositories_button(cx));
        let toolbar = match self.repository_filter_button(cx) {
            Some(clear) => toolbar.leading(clear),
            None => toolbar,
        };
        let toolbar = match self.picker_in_toolbar() {
            true => toolbar.leading(self.account_button(cx)),
            false => toolbar,
        };
        toolbar.trailing(Self::refresh_button(cx))
    }

    /// Whether the account picker sits in the toolbar: while the navigator,
    /// whose footer holds it otherwise, is hidden, and more than one account
    /// is signed in.
    fn picker_in_toolbar(&self) -> bool {
        !self.navigator_open && self.accounts.len() > 1
    }

    /// The view menu's control, named after the current view.
    fn view_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = self.view_title();
        let icon = github_section_icon(self.section);
        let button = menu_button("gh-view-button", icon, title, false)
            .accessibility_label(format!("View: {title}"))
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_view_menu(window, cx);
            }));
        self.menu_anchors.trigger(VIEW_ANCHOR, button, cx)
    }

    /// Shows Favourites only as the control's latched state.
    fn repositories_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let only = self.favourites.only;
        let name = match only {
            true => "Repositories, Favourites only on",
            false => "Repositories",
        };
        let button = menu_button("gh-repos-button", IconName::Star, "Repositories", only)
            .accessibility_label(name)
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_repositories(None, window, cx);
            }));
        self.menu_anchors.trigger(favourites::ANCHOR, button, cx)
    }

    /// The account picker, shown when more than one account is signed in.
    fn account_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.account.clone().unwrap_or_else(|| "…".into());
        let login = short_login(&account);
        let button = menu_button("gh-account-button", IconName::User, login, false)
            .accessibility_label(format!("GitHub account: {login}"))
            .on_click(cx.listener(|this, _, window, cx| {
                this.switch_account(&SwitchAccount, window, cx);
            }));
        self.menu_anchors.trigger("gh-account", button, cx)
    }

    /// Loads every view again, as `r` does.
    fn refresh_button(cx: &Context<Self>) -> AnyElement {
        icon_button("gh-refresh", Intent::Quiet, IconName::RefreshCw, "Refresh")
            .on_click(cx.listener(|this, _, window, cx| {
                this.reload(window, cx);
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Load, Signed, ViewCount, counts_line, dock_shown, dock_value, status_line};

    #[test]
    fn a_dock_shows_until_it_is_hidden() {
        assert!(dock_shown(None));
        assert!(dock_shown(Some("1")));
        assert!(!dock_shown(Some("0")));
        assert!(dock_shown(Some(dock_value(true))));
        assert!(!dock_shown(Some(dock_value(false))));
    }

    #[test]
    fn the_status_line_and_the_footer_name_the_account() {
        let signed = Signed::In("octo-dev");
        assert_eq!(signed.scope_line(), "GitHub · Signed in as octo-dev");
        assert_eq!(signed.footer_line(), "Signed in to GitHub as octo-dev");
        let unavailable = Signed::Unavailable;
        assert_eq!(unavailable.scope_line(), "GitHub · Account unavailable");
        assert_eq!(unavailable.footer_line(), "GitHub account unavailable");
        let checking = Signed::Checking;
        assert_eq!(checking.scope_line(), "GitHub · Checking account…");
        assert_eq!(checking.footer_line(), "Checking GitHub account…");
        let unknown = Signed::Unknown;
        assert_eq!(unknown.scope_line(), "GitHub");
        assert_eq!(unknown.footer_line(), "GitHub");
    }

    #[test]
    fn a_view_count_says_how_the_last_load_went_without_colour() {
        let count = |total: Option<u32>, load: Load| ViewCount { total, load };
        let ready = count(Some(3), Load::Ready);
        assert_eq!(ready.mark(), "3");
        assert_eq!(ready.words(), "3 items");
        let loading = count(Some(3), Load::Loading);
        assert_eq!(loading.mark(), "3 …");
        assert_eq!(loading.words(), "3 items, loading");
        let failed = count(Some(3), Load::Failed);
        assert_eq!(failed.mark(), "3 !");
        assert_eq!(failed.words(), "3 items, load failed");
        let never_loaded = count(None, Load::Failed);
        assert_eq!(never_loaded.mark(), "!");
        assert_eq!(never_loaded.words(), "Load failed");
        let on_its_way = count(None, Load::Loading);
        assert_eq!(on_its_way.mark(), "…");
        assert_eq!(on_its_way.words(), "loading");
    }

    #[test]
    fn the_status_line_puts_the_account_before_the_counts() {
        assert_eq!(counts_line(8, 8, 4, "open PRs"), "8 open PRs · 4 repos");
        assert_eq!(counts_line(1, 1, 1, "open PRs"), "1 open PRs · 1 repo");
        let more = counts_line(50, 73, 9, "open PRs");
        assert_eq!(more, "50 of 73 open PRs · 9 repos");
        let scope = Signed::In("octo-dev").scope_line();
        let counted = status_line(&scope, Some("8 open PRs · 4 repos"));
        let expected = "GitHub · Signed in as octo-dev · 8 open PRs · 4 repos";
        assert_eq!(counted, expected);
        assert_eq!(status_line(&scope, None), scope);
    }
}
