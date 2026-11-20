//! The toolbar's project picker uses the
//! shared filterable menu, as the PRs board's account picker is.

use gpui_kit::{AnyElement, Context, Window, component::Disableable as _};
use subspace_api_client::model::Project;
use subspace_framework::dashboard::action::menu_button;
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::lucide::IconName;

use crate::Issues;
use crate::model::ProjectScope;

const PROJECT_ANCHOR: &str = "issues-project";

/// The toolbar's project control: `NET · Subspace API`, or All projects.
pub(crate) fn project_button(issues: &Issues, cx: &mut Context<Issues>) -> AnyElement {
    let label = if crate::initiative::no_initiative_projects(issues) {
        "No projects".to_owned()
    } else {
        project_label(&issues.project, &issues.projects)
    };
    let button = menu_button(
        "issues-project-button",
        IconName::Folder,
        label.clone(),
        false,
    )
    .accessibility_label(format!("Project: {label}"))
    .disabled(crate::initiative::no_initiative_projects(issues))
    .on_click(cx.listener(|this, _, window, cx| open_projects(this, window, cx)));
    issues.menu_anchors.trigger(PROJECT_ANCHOR, button, cx)
}

fn open_projects(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let contents = if teams() {
        MenuContents::new("Team", "Filter teams", menu_options(issues))
    } else {
        MenuContents::new("Project", "Filter projects", menu_options(issues))
    };
    open(issues, contents, window, cx);
}

pub(crate) fn refresh_project_menu(issues: &Issues, cx: &mut Context<Issues>) {
    if let Some(menu) = issues.menu.as_ref()
        && matches!(menu.read(cx).label().as_str(), "Project" | "Team")
    {
        menu.update(cx, |menu, cx| menu.set_options(menu_options(issues), cx));
    }
}

fn menu_options(issues: &Issues) -> Vec<MenuOption> {
    scoped_options(issues)
        .map(|(scope, label)| {
            let checked = scope == issues.project;
            MenuOption::new(scope.key().to_owned(), label).checked(checked)
        })
        .collect()
}

fn open(
    issues: &mut Issues,
    contents: MenuContents,
    window: &mut Window,
    cx: &mut Context<Issues>,
) {
    let anchor = issues.menu_anchors.anchor(PROJECT_ANCHOR, cx);
    let Some(menu) = issues.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe(&menu, move |this, _, event: &MenuEvent, cx| {
        if matches!(event, MenuEvent::Query(_)) {
            return;
        }
        this.menu = None;
        if let MenuEvent::Pick(value) = event {
            this.set_project(ProjectScope::from_key(value), cx);
        }
        cx.notify();
    })
    .detach();
    issues.menu = Some(menu);
    cx.notify();
}

/// Whether the desktop's projects are NETT issue teams, so Issues names them so.
pub(crate) fn teams() -> bool {
    subspace_api_client::backend() == subspace_api_client::Backend::Nett
}

fn all_label() -> &'static str {
    if teams() { "All teams" } else { "All projects" }
}

/// `KEY · Name` for a known project, the key alone before the projects load.
pub(crate) fn project_label(scope: &ProjectScope, projects: &[Project]) -> String {
    match scope {
        ProjectScope::AllProjects => all_label().to_owned(),
        ProjectScope::Project(key) => projects
            .iter()
            .find(|project| &project.key == key)
            .map_or_else(
                || key.clone(),
                |project| format!("{} · {}", project.key, project.name),
            ),
    }
}

pub(crate) fn scoped_options(issues: &Issues) -> impl Iterator<Item = (ProjectScope, String)> + '_ {
    project_options(&issues.projects)
        .into_iter()
        .filter(|(scope, _)| crate::initiative::project_in_scope(issues, scope))
}

/// Active projects, followed by All projects.
pub(crate) fn project_options(projects: &[Project]) -> Vec<(ProjectScope, String)> {
    let mut options: Vec<_> = projects
        .iter()
        .filter(|project| project.archived_at.is_none())
        .map(|project| {
            (
                ProjectScope::Project(project.key.clone()),
                format!("{} · {}", project.key, project.name),
            )
        })
        .collect();
    options.push((ProjectScope::AllProjects, all_label().into()));
    options
}

#[cfg(test)]
#[path = "tests/pickers.rs"]
mod tests;
