//! The Space's workspace around the issue list: the navigator, the pane
//! with its tab strip and toolbar over the list (or the screen that stands
//! in for it), and the inspector's peek of the selected issue. The issue
//! page shares the navigator. The dock switches are the status bar's
//! (`Dashboard::docks`).

mod navigation;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, div, px,
};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::tabs::{self, TabSpec};
use subspace_framework::dashboard::tokens::ButtonRole;
use subspace_framework::dashboard::workspace::{Pane, Toolbar, Workspace};
use subspace_framework::dashboard::{Tokens, fields};
use subspace_framework::lucide::IconName;

use crate::docks::Docks;
use crate::filter_menu::{self, FilterMenu};
use crate::model::SubIssues;
use crate::navigator::View;
use crate::{Issues, initiative, keys, pickers, view_menu};

pub(crate) use navigation::navigator;

/// The search field's width in the toolbar.
const SEARCH_WIDTH_PX: f32 = 208.;

/// What the pane shows above its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Head {
    /// The tab strip, and the list's search, filters and actions.
    List,
    /// The tab strip, and the activity feed's actions.
    Activity,
    /// Nothing: a screen stands in for the whole board, as while signing in.
    Bare,
}

/// The list's workspace, with `content` in the pane under `head`.
pub(super) fn list(
    issues: &Issues,
    content: AnyElement,
    head: Head,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let shown = issues.shown_docks(&Tokens::current(cx));
    let pane = Pane::new("issues-pane")
        .when_some(toolbar(issues, head, shown, cx), |pane, toolbar| {
            // A screen standing in for the list names no view.
            pane.when(head != Head::Bare, |pane| pane.tabs(view_tabs(issues)))
                .toolbar(toolbar)
        })
        .child(content);
    Workspace::new("issues-workspace")
        .when(shown.navigator, |workspace| {
            workspace.navigator(navigator(issues, cx))
        })
        .pane(pane)
        .when(shown.inspector, |workspace| {
            workspace.inspector(super::peek::inspector(issues, cx))
        })
        .into_any_element()
}

/// The tab strip names what is open: the list, as its view.
fn view_tabs(issues: &Issues) -> AnyElement {
    let view = View::current(issues.tab, issues.activity.visible);
    let bar = tabs::pane_tab_bar("issues-tab-bar")
        .selected_index(0)
        .child(tabs::tab(TabSpec::new(view.label(), view.icon())));
    div()
        .id("issues-tabs")
        .test_support()
        .child(bar)
        .into_any_element()
}

fn toolbar(issues: &Issues, head: Head, shown: Docks, cx: &mut Context<Issues>) -> Option<Toolbar> {
    match head {
        Head::List => Some(list_toolbar(issues, shown, cx)),
        Head::Activity => Some(activity_toolbar(issues, shown, cx)),
        Head::Bare => bare_toolbar(shown, || crate::organisation::button(issues, cx)),
    }
}

/// A screen standing in for the list has no toolbar, except that the NETT
/// organisation menu stays in reach while the navigator, which otherwise
/// holds it, is hidden: the screen asking for an organisation points at it.
fn bare_toolbar(
    shown: Docks,
    organisation: impl FnOnce() -> Option<AnyElement>,
) -> Option<Toolbar> {
    if shown.navigator {
        return None;
    }
    // Without a tab strip the toolbar is the pane's strip.
    organisation().map(|menu| Toolbar::new("issues-toolbar").strip().leading(menu))
}

/// Search and filters on the leading side; the sub-issue switch, Refresh
/// and New issue… trail. The count is the
/// status bar's.
fn list_toolbar(issues: &Issues, shown: Docks, cx: &mut Context<Issues>) -> Toolbar {
    scoped_toolbar(issues, shown, cx)
        .leading(search(issues, cx))
        .leading(filter_menu::button(FilterMenu::Assignee, issues, cx))
        .leading(filter_menu::button(FilterMenu::Priority, issues, cx))
        .leading(filter_menu::button(FilterMenu::More, issues, cx))
        .trailing(sub_issues_button(issues, cx))
        .trailing(refresh_button(cx))
        .when(issues.can_create_issue(), |toolbar| {
            toolbar.trailing(crate::editing::view::new_button(cx))
        })
}

/// Refresh and New issue… stay beside the activity feed.
fn activity_toolbar(issues: &Issues, shown: Docks, cx: &mut Context<Issues>) -> Toolbar {
    scoped_toolbar(issues, shown, cx)
        .trailing(refresh_button(cx))
        .when(issues.can_create_issue(), |toolbar| {
            toolbar.trailing(crate::editing::view::new_button(cx))
        })
}

/// The pane's toolbar, led by what the hidden navigator would hold.
fn scoped_toolbar(issues: &Issues, shown: Docks, cx: &mut Context<Issues>) -> Toolbar {
    scope_controls(issues, shown, cx)
        .into_iter()
        .fold(Toolbar::new("issues-toolbar"), |toolbar, control| {
            toolbar.leading(control)
        })
}

fn refresh_button(cx: &Context<Issues>) -> IconButton {
    icon_button(
        "issues-refresh",
        Intent::Quiet,
        IconName::RefreshCw,
        "Refresh",
    )
    .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx)))
}

/// While the navigator is hidden, what it holds stays in reach: the view
/// menu first, then the initiative, project and organisation pickers.
fn scope_controls(issues: &Issues, shown: Docks, cx: &mut Context<Issues>) -> Vec<AnyElement> {
    if shown.navigator {
        return Vec::new();
    }
    let mut controls = vec![view_menu::button(issues, cx)];
    controls.extend(initiative::button(issues, cx));
    controls.push(pickers::project_button(issues, cx));
    controls.extend(crate::organisation::button(issues, cx));
    controls
}

fn search(issues: &Issues, cx: &Context<Issues>) -> impl IntoElement {
    let search = fields::toolbar_search(&issues.search, "Search issues", cx);
    div()
        .key_context(keys::SEARCH_CONTEXT)
        .w(px(SEARCH_WIDTH_PX))
        .max_w_full()
        .child(search.id("issues-search"))
}

/// The action names the result, and its tooltip includes the hidden count.
fn sub_issues_button(issues: &Issues, cx: &Context<Issues>) -> impl IntoElement {
    let action = issues.sub_issues.action();
    let tooltip = super::hidden_subissues(issues)
        .map(|count| format!("{action} ({count} hidden)"))
        .unwrap_or_else(|| action.to_owned());
    action_button(
        "issues-sub-issues",
        Intent::Quiet,
        Some(IconName::ListTree),
        action,
    )
    .role(ButtonRole::Toolbar)
    .tooltip(tooltip)
    .latched(issues.sub_issues == SubIssues::Nested)
    .on_click(cx.listener(|this, _, _, cx| this.toggle_sub_issues(cx)))
}

#[cfg(test)]
mod tests {
    use gpui_kit::{IntoElement as _, div};

    use super::bare_toolbar;
    use crate::docks::Docks;

    const HIDDEN: Docks = Docks {
        navigator: false,
        inspector: true,
    };

    #[test]
    fn a_screen_keeps_the_organisation_menu_while_the_navigator_is_hidden() {
        let picker = || Some(div().into_any_element());
        assert!(bare_toolbar(HIDDEN, picker).is_some());
        assert!(
            bare_toolbar(Docks::default(), || panic!("the navigator holds the menu")).is_none(),
            "the navigator's header holds it"
        );
        assert!(
            bare_toolbar(HIDDEN, || None).is_none(),
            "no menu, as outside NETT or with one organisation, leaves no toolbar"
        );
    }
}
