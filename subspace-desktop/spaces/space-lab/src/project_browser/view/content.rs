//! The pane's content: the empty state, the New project form, or the open
//! project under its revision lines. Errors stay in a band beneath it.

use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Stateful, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, px,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::workspace;
use subspace_framework::dashboard::{Tokens, anchored, typography};

use super::super::ProjectBrowser;
use super::super::facts::{self, WorkerFacts};
use super::worker_facts;
use crate::project_service::ProjectPresentation;
use crate::shell::{self, ShellRequest, ViewTab};

/// The widest the empty state's column runs.
const WELCOME_WIDTH_PX: f32 = 560.;
const WELCOME_TITLE: &str = "Your native dashboards";
const WELCOME_LEAD: &str = "Open a project or choose New project. Source stays in ordinary \
     files, and no worker starts until you choose Start.";
const WELCOME_NOTE: &str = "Environment installation is separate. Use Environments to \
     configure and explicitly provision Python, Node or Bun.";
const CHANGES_NOT_RUNNING: &str = "Working changes are not running. Apply & Restart explicitly.";
const WORKER_FAILED: &str =
    "Worker failed. Last valid results remain visible; choose Restart to run again.";

impl ProjectBrowser {
    /// The New project form covers an open project until it is cancelled.
    pub(super) fn content(&self, cx: &mut Context<Self>, tokens: &Tokens) -> AnyElement {
        if self.creating {
            self.creation_form(cx, tokens).into_any_element()
        } else if let Some(project) = &self.presentation {
            self.project_content(project, tokens).into_any_element()
        } else {
            self.welcome(cx, tokens).into_any_element()
        }
    }

    /// The empty state: a column in the middle of the content that says what
    /// the Space is for and offers the two ways on.
    fn welcome(&self, cx: &mut Context<Self>, tokens: &Tokens) -> Div {
        let column = v_flex()
            .id("space-lab-welcome")
            .test_support()
            .w_full()
            .max_w(px(WELCOME_WIDTH_PX))
            .gap_2()
            .child(welcome_title(cx, tokens))
            .child(WELCOME_LEAD)
            .child(div().text_color(tokens.muted).child(WELCOME_NOTE))
            .child(self.welcome_actions(cx));
        v_flex()
            .size_full()
            .min_h_0()
            .items_center()
            .justify_center()
            .p(workspace::text_edge(tokens))
            .child(column)
    }

    fn welcome_actions(&self, cx: &mut Context<Self>) -> Div {
        let create = action_button(
            "project-new-welcome",
            Intent::Neutral,
            Some(IconName::Plus),
            shell::NEW_PROJECT,
        )
        .disabled(self.busy)
        .on_click(cx.listener(|this, _, window, cx| this.begin_creating(window, cx)));
        let environments = action_button(
            "space-lab-open-environments",
            Intent::Neutral,
            Some(IconName::Cpu),
            "Open Environments",
        )
        .on_click(cx.listener(|_, _, _, cx| {
            cx.emit(ShellRequest::Show(ViewTab::Environments));
        }));
        h_flex()
            .gap_2()
            .flex_wrap()
            .pt_2()
            .child(create)
            .child(environments)
    }

    fn creation_form(&self, cx: &mut Context<Self>, _: &Tokens) -> Stateful<Div> {
        v_flex().id("project-create-form").min_h_0().overflow_y_scroll().p_4().gap_4()
            .child("New project")
            .child(v_flex().gap_1().child("Project ID").child(fields::input(&self.project_id, InputRole::Configuration, "Project ID").id("project-create-id").disabled(self.busy)))
            .child(v_flex().gap_1().child("Configured environment")
                .child(anchored::select(&self.environment).small().accessibility_label("Project environment").disabled(self.busy)))
            .child("Creates a readable native dashboard example and source files. It does not provision packages, contact services or start code.")
            .when(self.environments.is_empty(), |form| form.child("No configured Python, Node or Bun environments. Open Environments to configure one."))
            .child(h_flex().gap_2()
                .child(action_button("project-create-cancel", Intent::Neutral, Some(IconName::X), "Cancel")
                    .disabled(self.busy).on_click(cx.listener(|this, _, _, cx| { this.creating = false; cx.notify(); })))
                .child(action_button("project-create", Intent::Primary, Some(IconName::Plus), "Create")
                    .disabled(self.busy || self.environments.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| this.create(window, cx)))))
    }

    /// The open project's revision lines, its dashboard and its diagnostics.
    /// Its name and its actions are in the toolbar.
    fn project_content(&self, project: &ProjectPresentation, tokens: &Tokens) -> Div {
        let worker = worker_facts(project);
        let edge = workspace::text_edge(tokens);
        v_flex()
            .size_full()
            .min_h_0()
            .gap_2()
            .child(revision_lines(project, worker, tokens))
            .child(div().flex_1().min_h_0().children(self.dashboard.clone()))
            .when(!project.diagnostics.is_empty(), |content| {
                content.child(
                    div()
                        .px(edge)
                        .pb_2()
                        .text_color(tokens.muted)
                        .child(project.diagnostics.clone()),
                )
            })
    }
}

fn welcome_title(cx: &App, tokens: &Tokens) -> Div {
    div()
        .font_family(typography::heading_font(cx))
        .font_weight(FontWeight::MEDIUM)
        .text_size(tokens.heading_font)
        .text_color(tokens.fg_strong)
        .child(WELCOME_TITLE)
}

/// Which revision is in the working files and which one runs, with what to
/// do when they differ or the worker failed.
fn revision_lines(project: &ProjectPresentation, worker: WorkerFacts, tokens: &Tokens) -> Div {
    v_flex()
        .px(workspace::text_edge(tokens))
        .pt_2()
        .gap_1()
        .text_color(tokens.muted)
        .child(facts::working_line(&project.working.revision))
        .child(facts::running_line(
            project.applied_revision.as_ref(),
            project.status,
        ))
        .when(worker.changed, |lines| lines.child(CHANGES_NOT_RUNNING))
        .when(worker.failed, |lines| lines.child(WORKER_FAILED))
}

/// An error stays in a band under the content until the next operation.
pub(super) fn error_band(error: &str, tokens: &Tokens) -> Stateful<Div> {
    div()
        .id("project-errors")
        .max_h_32()
        .overflow_y_scroll()
        .px(workspace::text_edge(tokens))
        .py_3()
        .text_color(tokens.red)
        .child(error.to_owned())
}
