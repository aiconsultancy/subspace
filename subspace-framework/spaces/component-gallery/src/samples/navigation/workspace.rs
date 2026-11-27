//! The parts of the workspace shell every Space shares: the navigator and
//! its rows, the pane's toolbar, the dock switches, the pane and the
//! inspector.
//!
//! `workspace::Workspace` is not here: its dock edges store the app-wide dock
//! widths when dragged, and the gallery must not write preferences.

use gpui_kit::component::h_flex;
use gpui_kit::component::input::InputState;
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Styled as _, Window, div, px,
};
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::fields;
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::row_menu::{GroupedMenu, MenuCommand, MenuGroup};
use subspace_dashboard::tabs::{self, TabSpec};
use subspace_dashboard::toolbar_label::{self, ToolbarLabel};
use subspace_dashboard::workspace::{self, Navigator, Pane, SpaceDock, Toolbar};
use subspace_dashboard::{Tokens, avatar, rich};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::catalog::Item;
use crate::samples::{Sample, bounded, ignore_click, sample, sized};

mod inspector;
pub(super) use inspector::{INSPECTOR, INSPECTOR_TAB};
#[cfg(test)]
pub(in crate::samples) use inspector::{inspector_tabs, inspectors};

const NAVIGATOR_SOURCE: &str = "board-kit/dashboard/src/workspace/navigator.rs";
const PANE_SOURCE: &str = "board-kit/dashboard/src/workspace/pane.rs";
const DOCKS_SOURCE: &str = "board-kit/dashboard/src/workspace/docks.rs";
const VIEWS_BOX: (f32, f32) = (240., 320.);
const NETWORKS_BOX: (f32, f32) = (240., 300.);
const TOOLBAR_WIDTH: f32 = 640.;
const NARROW_TOOLBAR_WIDTH: f32 = 360.;
const SEARCH_WIDTH: f32 = 160.;
const PANE_BOX: (f32, f32) = (640., 240.);
const SEARCH_LABEL: &str = "Search issues";
const PANE_TEXT: &str = "Content starts on the same text edge as the tab strip and the toolbar. Reading content, such as an issue's body, is no wider than the reading width.";

pub(super) const NAVIGATOR: Item = Item {
    name: "workspace::Navigator",
    source: NAVIGATOR_SOURCE,
    used_in: "Every Space's leading dock: the Space's name in the header, then workspace::nav_section labels and workspace::NavRow rows for its views, projects, networks or repositories.",
    samples: navigators,
    kit: None,
};

pub(super) const TOOLBAR: Item = Item {
    name: "workspace::Toolbar",
    source: PANE_SOURCE,
    used_in: "Every Space's pane toolbar: search and filters on the leading side, actions on the trailing side. Toolbar::strip makes it the pane's strip where there is no tab strip, as in Projects and Channels.",
    samples: toolbars,
    kit: None,
};

pub(super) const DOCK_SWITCH: Item = Item {
    name: "workspace::dock_switch",
    source: DOCKS_SOURCE,
    used_in: "The two ends of the shell's status bar, for the docks the Space on show reports through Dashboard::docks: the navigator's switch first, the inspector's last.",
    samples: dock_switches,
    kit: None,
};

pub(super) const PANE: Item = Item {
    name: "workspace::Pane",
    source: PANE_SOURCE,
    used_in: "Every Space's work area: a tab strip for what is open, a workspace::Toolbar, then the content.",
    samples: panes,
    kit: None,
};

/// The retained search inputs of the toolbar and pane samples.
pub(crate) struct Searches {
    pub(crate) toolbar: Entity<InputState>,
    pub(crate) pane: Entity<InputState>,
}

impl Searches {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let search = |window: &mut Window, cx: &mut App| {
            cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_LABEL))
        };
        Self {
            toolbar: search(window, cx),
            pane: search(window, cx),
        }
    }
}

pub(in crate::samples) fn navigators(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let cx = &*cx;
    let views = views_navigator(cx);
    let networks = networks_navigator(tokens, cx);
    vec![
        sample(
            "A badge, a header action and two sections: selected, plain, counted and quiet rows",
            scoped("gallery-sample:navigator-views", VIEWS_BOX, tokens, views),
        ),
        sample(
            "A strong parent, indented rows with a right-click menu, a trailing mark, a leading element and a footer",
            scoped(
                "gallery-sample:navigator-networks",
                NETWORKS_BOX,
                tokens,
                networks,
            ),
        ),
    ]
}

/// A fixed-size box with its own element id. A navigator names its own
/// elements, so two on one card need separate id scopes.
fn scoped(id: &'static str, size: (f32, f32), tokens: &Tokens, part: impl IntoElement) -> Div {
    bounded(size, tokens, div().id(id).size_full().child(part))
}

/// A navigator row whose click does nothing outside the gallery.
fn row(key: &'static str, label: &'static str) -> workspace::NavRow {
    workspace::NavRow::new(key, label).on_click(ignore_click)
}

/// A channel under its network.
fn channel(key: &'static str, label: &'static str) -> workspace::NavRow {
    row(key, label)
        .icon(IconName::Hash)
        .indent(1)
        .menu(channel_menu)
}

/// A channel's right-click menu, built from the standard groups.
fn channel_menu(_: &mut Window, _: &mut App) -> GroupedMenu {
    let open = MenuCommand::new(MenuGroup::Open, "Open", IconName::ArrowRight, |_, _| {});
    let leave = MenuCommand::new(
        MenuGroup::Remove,
        "Leave channel",
        IconName::LogOut,
        |_, _| {},
    );
    GroupedMenu::new().with(open).with(leave)
}

fn views_navigator(cx: &App) -> Navigator {
    let new_issue = action::icon_button(
        "gallery-sample:navigator-new",
        Intent::Quiet,
        IconName::Plus,
        "New issue",
    )
    .on_click(ignore_click);
    Navigator::new("Issues")
        .badge("Alpha")
        .action(new_issue)
        .child(workspace::nav_section("Views", cx))
        .child(
            row("gallery-sample:navrow-mine", "My issues")
                .icon(IconName::User)
                .count("4")
                .selected(true),
        )
        .child(row("gallery-sample:navrow-active", "Active").icon(IconName::CircleDot))
        .child(
            row("gallery-sample:navrow-backlog", "Backlog")
                .icon(IconName::Inbox)
                .count("12"),
        )
        .child(
            row("gallery-sample:navrow-done", "Done")
                .icon(IconName::CircleCheck)
                .quiet(true),
        )
        .child(workspace::nav_section("Projects", cx))
        .child(
            row("gallery-sample:navrow-harbour", "Harbour")
                .icon(IconName::Folder)
                .count("7"),
        )
        .child(
            row("gallery-sample:navrow-lighthouse", "Lighthouse")
                .icon(IconName::Folder)
                .quiet(true),
        )
}

fn networks_navigator(tokens: &Tokens, cx: &App) -> Navigator {
    let unread = tabs::unread_dot(tokens);
    let person = avatar::member("Ada Example", tokens);
    let account = rich::plain_text("gallery-sample:navigator-account", "Signed in as octo-dev");
    Navigator::new("Channels")
        .child(workspace::nav_section("Networks", cx))
        .child(
            row("gallery-sample:navrow-network", "Example network")
                .strong(true)
                .count("Connected"),
        )
        .child(channel("gallery-sample:navrow-general", "general"))
        .child(channel("gallery-sample:navrow-ops", "ops").trailing(unread))
        .child(workspace::nav_section("Direct messages", cx))
        .child(row("gallery-sample:navrow-ada", "Ada Example").leading(person))
        .footer(account)
}

pub(in crate::samples) fn toolbars(
    gallery: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let search = &gallery.retained.searches.toolbar;
    let shown = full_toolbar("gallery-sample:toolbar-shown", search, cx);
    vec![
        sample(
            "Search, filters, quiet text and the main action",
            sized(TOOLBAR_WIDTH, shown),
        ),
        sample(
            "Navigator hidden: a view menu leads",
            sized(TOOLBAR_WIDTH, navigator_hidden_toolbar()),
        ),
        sample(
            "Narrow: leading items wrap, trailing items stay on the first line",
            sized(NARROW_TOOLBAR_WIDTH, narrow_toolbar()),
        ),
        sample(
            "Toolbar::strip: heads a pane with no tab strip, exactly the strip height",
            sized(TOOLBAR_WIDTH, strip_toolbar()),
        ),
    ]
}

/// A filter's menu trigger. Its id is unique within one toolbar.
fn filter(id: &'static str, label: &'static str) -> IconButton {
    action::menu_button(id, IconName::ListFilter, label, false).on_click(ignore_click)
}

/// The menu that names the current view while the navigator is hidden.
fn view_menu() -> IconButton {
    action::menu_button(
        "gallery-sample:toolbar-view",
        IconName::ListChecks,
        "My issues",
        false,
    )
    .on_click(ignore_click)
}

/// A filter named by its label alone, as Assignee and Priority are.
fn plain_filter(id: &'static str, label: &'static str) -> IconButton {
    action::filter_button(id, label, false).on_click(ignore_click)
}

/// The Space's one main action: the toolbar's only primary button.
fn new_issue() -> IconButton {
    action::main_action("gallery-sample:toolbar-new", IconName::Plus, "New issue…")
        .on_click(ignore_click)
}

fn search_field(state: &Entity<InputState>, cx: &App) -> Div {
    let input = fields::toolbar_search(state, SEARCH_LABEL, cx);
    sized(SEARCH_WIDTH, input)
}

/// Search, filters, quiet text and the main action.
fn full_toolbar(id: &'static str, search: &Entity<InputState>, cx: &App) -> Toolbar {
    Toolbar::new(id)
        .leading(search_field(search, cx))
        .leading(plain_filter("gallery-sample:toolbar-assignee", "Assignee"))
        .leading(filter("gallery-sample:toolbar-status", "Filters"))
        .trailing(grouping(cx))
        .trailing(new_issue())
}

/// Quiet text on the trailing side, before the actions.
fn grouping(cx: &App) -> impl IntoElement + use<> {
    let text = ToolbarLabel {
        icon: None,
        text: "Grouped by state".into(),
        accessible: "Grouped by state".into(),
    };
    toolbar_label::label("gallery-sample:toolbar-grouping", text, cx)
}

fn navigator_hidden_toolbar() -> Toolbar {
    Toolbar::new("gallery-sample:toolbar-hidden")
        .leading(view_menu())
        .leading(filter("gallery-sample:toolbar-status", "Status"))
        .trailing(new_issue())
}

/// A pane with no tab strip, such as Channels', heads it with its toolbar.
fn strip_toolbar() -> Toolbar {
    Toolbar::new("gallery-sample:toolbar-strip")
        .strip()
        .leading(filter("gallery-sample:toolbar-status", "Status"))
        .trailing(new_issue())
}

fn narrow_toolbar() -> Toolbar {
    Toolbar::new("gallery-sample:toolbar-narrow")
        .leading(filter("gallery-sample:toolbar-status", "Status"))
        .leading(filter("gallery-sample:toolbar-assignee", "Assignee"))
        .leading(filter("gallery-sample:toolbar-priority", "Priority"))
        .trailing(new_issue())
}

pub(in crate::samples) fn dock_switches(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Both docks shown: quiet panel glyphs in the accent, no fill",
            switch_pair("gallery-sample:dock-switches-shown", true),
        ),
        sample(
            "Both docks hidden: the glyphs return to the quiet colour",
            switch_pair("gallery-sample:dock-switches-hidden", false),
        ),
    ]
}

/// The navigator's and the inspector's switches, in their own id scope: each
/// switch keeps the one id it has in every Space.
fn switch_pair(id: &'static str, shown: bool) -> impl IntoElement {
    let switch = |dock| workspace::dock_switch(dock, shown, ignore_click);
    h_flex().id(id).gap_2().children(SpaceDock::ALL.map(switch))
}

pub(in crate::samples) fn panes(
    gallery: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let search = &gallery.retained.searches.pane;
    let list = tabs::tab(TabSpec::new("Issues", IconName::ListChecks));
    let issue = tabs::tab(TabSpec::new("SUB-12", IconName::CircleDot)).on_close(ignore_click);
    let open = tabs::pane_tab_bar("gallery-sample:pane-tabs")
        .selected_index(0)
        .child(list)
        .child(issue);
    let pane = Pane::new("gallery-sample:pane")
        .tabs(open)
        .toolbar(full_toolbar("gallery-sample:pane-toolbar", search, cx))
        .child(reading(tokens));
    vec![sample(
        "A tab strip for what is open, a toolbar and reading content",
        bounded(PANE_BOX, tokens, pane),
    )]
}

/// Reading content: no wider than the reading width, on the text edge.
fn reading(tokens: &Tokens) -> Div {
    div()
        .w_full()
        .max_w(px(workspace::READING_WIDTH_PX))
        .px(workspace::text_edge(tokens))
        .py_3()
        .child(rich::plain_text("gallery-sample:pane-content", PANE_TEXT))
}
