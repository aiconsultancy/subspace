//! The shared initiative picker and its project scope in Issues.

use gpui_kit::{App, Context, Entity, Subscription, Window};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::model::Project;

use crate::{Issues, model::ProjectScope};

mod picker;
pub(crate) use picker::{button, command};

pub(crate) fn connect(
    window: &Window,
    cx: &mut Context<Issues>,
) -> (Entity<Catalog>, Option<String>, Subscription) {
    let catalog = Catalog::shared(cx);
    let selected = selected_id(catalog.read(cx));
    let window = window.window_handle();
    let revision = std::cell::Cell::new(catalog.read(cx).revision());
    let subscription = cx.observe(&catalog, move |board, catalog, cx| {
        let catalog = catalog.read(cx);
        let initiative = selected_id(catalog);
        if revision.replace(catalog.revision()) != catalog.revision() {
            dismiss_scope_menu(board, window, cx);
        }
        initiative_changed(board, initiative, cx);
    });
    (catalog, selected, subscription)
}

fn selected_id(catalog: &Catalog) -> Option<String> {
    catalog.selected().map(|initiative| initiative.id.clone())
}

fn dismiss_scope_menu(board: &Issues, window: gpui_kit::AnyWindowHandle, cx: &mut Context<Issues>) {
    let Some(menu) = board.menu.as_ref() else {
        return;
    };
    if !matches!(menu.read(cx).label().as_str(), "Initiative" | "Project") {
        return;
    }
    let menu_id = menu.entity_id();
    let owner = board.weak.clone();
    let app: &mut App = cx;
    app.defer(move |cx| {
        _ = window.update(cx, |_, window, cx| {
            _ = owner.update(cx, |board, cx| {
                if board
                    .menu
                    .as_ref()
                    .is_some_and(|menu| menu.entity_id() == menu_id)
                {
                    board.menu_anchors.dismiss(window, cx);
                    board.menu = None;
                    cx.notify();
                }
            });
        });
    });
}

#[cfg(not(test))]
pub(crate) fn refresh(issues: &Issues, cx: &mut Context<Issues>) {
    issues.initiatives.update(cx, Catalog::refresh);
}

fn initiative_changed(issues: &mut Issues, initiative: Option<String>, cx: &mut Context<Issues>) {
    if issues.initiative_id == initiative {
        cx.notify();
        return;
    }
    let current = match (no_initiative_projects(issues), initiative.is_none()) {
        (true, true) => saved_project(cx),
        (true, false) => None,
        (false, _) => Some(issues.project.clone()),
    };
    issues.initiative_id = initiative;
    let choice = scoped_project(issues, current);
    if issues.project != choice {
        issues.set_list_project(choice);
    }
    if issues.hub_changes.is_some() || issues.hub.is_none() {
        issues.rescope(cx);
    } else {
        clear_empty_scope(issues);
        issues.cancel_search_fallback();
        issues.clear_list_scope();
        cx.notify();
    }
}

pub(crate) fn saved_project(cx: &App) -> Option<ProjectScope> {
    subspace_framework::dashboard::prefs::get(cx, crate::board::PROJECT_PREF)
        .map(|key| ProjectScope::from_key(&key))
}

pub(crate) fn scoped_project(issues: &Issues, current: Option<ProjectScope>) -> ProjectScope {
    match &issues.initiative_id {
        Some(id) => initiative_project(&issues.projects, id, current),
        None => crate::model::chosen_project(current, &issues.projects),
    }
}

pub(crate) fn project_in_scope(issues: &Issues, scope: &ProjectScope) -> bool {
    let Some(id) = issues.initiative_id.as_deref() else {
        return true;
    };
    active_projects(&issues.projects, id).any(|project| project.key == scope.key())
}

pub(crate) fn no_initiative_projects(issues: &Issues) -> bool {
    issues.initiative_id.is_some() && issues.project == ProjectScope::AllProjects
}

fn active_projects<'a>(
    projects: &'a [Project],
    initiative: &'a str,
) -> impl Iterator<Item = &'a Project> {
    projects.iter().filter(|project| {
        project.archived_at.is_none() && project.initiative_id.as_deref() == Some(initiative)
    })
}

fn initiative_project(
    projects: &[Project],
    initiative: &str,
    current: Option<ProjectScope>,
) -> ProjectScope {
    let mut projects = active_projects(projects, initiative);
    let first = projects.next();
    if let Some(ProjectScope::Project(key)) = current
        && first
            .into_iter()
            .chain(projects)
            .any(|project| project.key == key)
    {
        return ProjectScope::Project(key);
    }
    first.map_or(ProjectScope::AllProjects, |project| {
        ProjectScope::Project(project.key.clone())
    })
}

/// An empty, loaded initiative has no list or Activity query to keep live.
pub(crate) fn clear_empty_scope(issues: &Issues) {
    if issues.is_ready()
        && no_initiative_projects(issues)
        && let Some(hub) = issues.hub.as_ref()
    {
        hub.clear_project_scope();
    }
}
