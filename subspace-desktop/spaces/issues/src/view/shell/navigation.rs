//! The navigator and its rows, shared by the list and the issue page.

use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _};
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::workspace::{self, NavRow, Navigator};
use subspace_framework::lucide::IconName;

use crate::model::ProjectScope;
use crate::navigator::{self, Entry, View};
use crate::{Issues, initiative, pickers};

/// The navigator: the Space's name, with the NETT organisation and New
/// issue in its header, then the views, the projects in scope and the
/// initiatives.
pub(crate) fn navigator(issues: &Issues, cx: &mut Context<Issues>) -> Navigator {
    let mut navigator = Navigator::new("Issues");
    if let Some(organisation) = crate::organisation::button(issues, cx) {
        navigator = navigator.action(organisation);
    }
    if issues.can_create_issue() {
        navigator = navigator.action(new_issue(cx));
    }
    navigator
        .child(workspace::nav_section("Views", cx))
        .children(view_rows(issues, cx))
        .children(project_rows(issues, cx))
        .children(initiative_rows(issues, cx))
}

fn new_issue(cx: &Context<Issues>) -> impl IntoElement {
    icon_button("issues-nav-new", Intent::Quiet, IconName::Plus, "New issue")
        .on_click(cx.listener(|this, _, window, cx| this.open_issue_editor(false, window, cx)))
}

/// My issues, Active, Backlog, All issues and Activity, the current one
/// selected. A row makes the same change as its key, `1` to `5`.
fn view_rows(issues: &Issues, cx: &Context<Issues>) -> Vec<NavRow> {
    let current = View::current(issues.tab, issues.activity.visible);
    navigator::views(current, issues.listing.my_issues_count)
        .into_iter()
        .map(|entry| {
            let view = entry.view;
            let row = NavRow::new(format!("issues-nav-view:{}", view.key()), view.label())
                .icon(view.icon())
                .selected(entry.selected)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.choose_view(view, window, cx);
                }));
            match entry.count {
                Some(count) => row.count(count),
                None => row,
            }
        })
        .collect()
}

/// The projects the project picker offers, under Projects (Teams in NETT).
fn project_rows(issues: &Issues, cx: &Context<Issues>) -> Vec<AnyElement> {
    let entries = navigator::projects(pickers::scoped_options(issues), &issues.project);
    if entries.is_empty() {
        return Vec::new();
    }
    let title = if pickers::teams() {
        "Teams"
    } else {
        "Projects"
    };
    let mut rows = vec![workspace::nav_section(title, cx).into_any_element()];
    rows.extend(
        entries
            .into_iter()
            .map(|entry| project_row(entry, cx).into_any_element()),
    );
    rows
}

fn project_row(entry: Entry, cx: &Context<Issues>) -> NavRow {
    let project = ProjectScope::from_key(&entry.value);
    let icon = if project == ProjectScope::AllProjects {
        IconName::Folders
    } else {
        IconName::Folder
    };
    NavRow::new(format!("issues-nav-project:{}", entry.value), entry.label)
        .icon(icon)
        .selected(entry.selected)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.choose_project(project.clone(), window, cx);
        }))
}

/// The initiative picker's choices, once there is one to make.
fn initiative_rows(issues: &Issues, cx: &Context<Issues>) -> Vec<AnyElement> {
    let options = issues
        .initiatives
        .read(cx)
        .menu(initiative::CLEAR_LABEL)
        .options;
    let entries = navigator::initiatives(&options);
    if entries.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![workspace::nav_section("Initiatives", cx).into_any_element()];
    rows.extend(
        entries
            .into_iter()
            .map(|entry| initiative_row(entry, cx).into_any_element()),
    );
    rows
}

fn initiative_row(entry: Entry, cx: &Context<Issues>) -> NavRow {
    let value = entry.value.clone();
    NavRow::new(
        format!("issues-nav-initiative:{}", entry.value),
        entry.label,
    )
    .icon(IconName::Users)
    .selected(entry.selected)
    .on_click(cx.listener(move |this, _, window, cx| {
        this.choose_initiative(&value, window, cx);
    }))
}
