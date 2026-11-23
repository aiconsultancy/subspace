//! The project browser's workspace: the shared navigator with the project
//! list, and a pane whose tab, toolbar and content follow what is open.

mod content;
mod toolbar;

use std::rc::Rc;

use gpui_kit::component::{Disableable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Render, Stateful,
    Styled as _, Window, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabBar, TabSpec};
use subspace_framework::dashboard::workspace::{self, NavRow, Navigator, Pane, Workspace};

use super::ProjectBrowser;
use super::facts::{Opened, WorkerFacts};
use crate::project::LoadedProject;
use crate::project_service::ProjectPresentation;
use crate::shell::{self, OnShow, Shell, ShellRequest, ViewTab};

impl Render for ProjectBrowser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let pane = self.pane(cx, &tokens);
        let workspace = Workspace::new("space-lab-workspace").pane(pane);
        let workspace = if self.shell.navigator {
            workspace.navigator(self.navigator(cx, &tokens))
        } else {
            workspace
        };
        div()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .child(workspace)
    }
}

impl ProjectBrowser {
    /// Takes the owner's shell facts: the views the switch offers and
    /// whether the navigator shows.
    pub(crate) fn set_shell(&mut self, shell: Shell, cx: &mut Context<Self>) {
        self.shell = shell;
        cx.notify();
    }

    /// What is open in the pane.
    fn opened(&self) -> Opened {
        let project = self
            .presentation
            .as_ref()
            .map(|project| project.working.definition.title.as_str());
        Opened::of(self.creating, project)
    }

    /// Opens a listed project and leaves the New project form. A pending
    /// operation keeps what is open.
    fn open_listed(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.creating = false;
        self.open(id, window, cx);
    }

    fn navigator(&self, cx: &mut Context<Self>, tokens: &Tokens) -> Navigator {
        let on_show = on_show(cx);
        shell::navigator(self.shell, ViewTab::Projects, on_show, cx)
            .action(
                shell::new_project_action()
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.begin_creating(window, cx))),
            )
            .child(self.project_list(cx, tokens))
            .footer(self.refresh_action(cx))
    }

    /// The projects, or a quiet line while there are none.
    fn project_list(&self, cx: &mut Context<Self>, tokens: &Tokens) -> Stateful<Div> {
        let list = v_flex().id("projects-list").w_full().flex_shrink_0();
        if self.projects.is_empty() {
            return list.when(!self.busy, |list| list.child(no_projects(tokens)));
        }
        let rows = self
            .projects
            .iter()
            .map(|project| self.project_row(project, cx))
            .collect::<Vec<_>>();
        list.children(rows)
    }

    fn project_row(&self, project: &LoadedProject, cx: &mut Context<Self>) -> NavRow {
        let id = project.definition.id.clone();
        let title = project.definition.title.clone();
        let open = !self.creating && self.selected_id().as_deref() == Some(id.as_str());
        NavRow::new(format!("open-project-{id}"), title)
            .icon(IconName::LayoutDashboard)
            .selected(open)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_listed(id.clone(), window, cx);
            }))
    }

    fn refresh_action(&self, cx: &mut Context<Self>) -> IconButton {
        action_button(
            "projects-refresh",
            Intent::Quiet,
            Some(IconName::RefreshCw),
            "Refresh projects",
        )
        .disabled(self.busy)
        .on_click(cx.listener(|this, _, window, cx| this.refresh_catalog(window, cx)))
    }

    fn pane(&self, cx: &mut Context<Self>, tokens: &Tokens) -> Pane {
        let opened = self.opened();
        let content = self.content(cx, tokens);
        let errors = self
            .error
            .as_deref()
            .map(|error| content::error_band(error, tokens));
        Pane::new("space-lab-pane")
            .tabs(open_tab(&opened))
            .toolbar(self.toolbar(&opened, cx))
            .child(div().flex_1().min_h_0().child(content))
            .children(errors)
    }
}

/// Asks the owner of the shell facts to show another view.
fn on_show(cx: &mut Context<ProjectBrowser>) -> OnShow {
    let owner = cx.weak_entity();
    Rc::new(move |view: ViewTab, _: &mut Window, cx: &mut App| {
        _ = owner.update(cx, |_, cx| cx.emit(ShellRequest::Show(view)));
    })
}

/// What the open project's worker allows.
fn worker_facts(project: &ProjectPresentation) -> WorkerFacts {
    WorkerFacts::new(
        project.status,
        project.applied_revision.as_ref(),
        &project.working.revision,
    )
}

/// The pane's one tab, which names what is open in it.
fn open_tab(opened: &Opened) -> TabBar {
    let tab = TabSpec::new(opened.tab_title().to_owned(), opened.icon());
    dashboard_tabs::pane_tab_bar("space-lab-open")
        .selected_index(0)
        .child(dashboard_tabs::tab(tab))
}

/// The navigator's line while there are no projects.
fn no_projects(tokens: &Tokens) -> Div {
    div()
        .px(workspace::text_edge(tokens))
        .py_1()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child("No projects yet")
}
