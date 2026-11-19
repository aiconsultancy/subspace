use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Render, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, anchored};

use super::ProjectBrowser;
use crate::project_runtime::SessionStatus;

impl Render for ProjectBrowser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let body = if self.creating {
            self.creation_form(cx, &tokens).into_any_element()
        } else if self.presentation.is_some() {
            self.project_content(cx, &tokens).into_any_element()
        } else {
            v_flex().p_4().gap_2()
                .child("Your native dashboards")
                .child("Open a project or choose New project. Source stays in ordinary files, and no worker starts until you choose Start.")
                .child("Environment installation is separate. Use the Environments tab to configure and explicitly provision Python, Node or Bun.")
                .into_any_element()
        };
        h_flex()
            .items_stretch()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .child(self.sidebar(cx, &tokens))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(div().flex_1().min_h_0().child(body))
                    .children(self.error.as_ref().map(|error| {
                        div()
                            .id("project-errors")
                            .max_h_32()
                            .overflow_y_scroll()
                            .p_3()
                            .text_color(tokens.red)
                            .child(error.clone())
                    }))
                    .child(
                        div()
                            .p_3()
                            .border_t_1()
                            .border_color(tokens.line)
                            .text_color(tokens.muted)
                            .child(if self.busy {
                                "Working…".into()
                            } else {
                                self.status.clone()
                            }),
                    ),
            )
    }
}

impl ProjectBrowser {
    fn sidebar(&self, cx: &mut Context<Self>, tokens: &Tokens) -> Div {
        v_flex()
            .w_56()
            .min_w_40()
            .max_w_72()
            .flex_shrink_0()
            .min_h_0()
            .border_r_1()
            .border_color(tokens.line)
            .child(
                v_flex().p_3().gap_2().child("Projects").child(
                    action_button(
                        "project-new",
                        Intent::Neutral,
                        Some(IconName::Plus),
                        "New project",
                    )
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.creating = true;
                        this.project_id
                            .update(cx, |input, cx| input.focus(window, cx));
                        cx.notify();
                    })),
                ),
            )
            .child(self.project_list(cx))
            .child(
                div().p_3().child(
                    action_button(
                        "projects-refresh",
                        Intent::Quiet,
                        Some(IconName::RefreshCw),
                        "Refresh projects",
                    )
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.refresh_catalog(window, cx))),
                ),
            )
    }

    fn project_list(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        v_flex()
            .id("projects-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .gap_2()
            .when(self.projects.is_empty() && !self.busy, |list| {
                list.child("No projects yet")
            })
            .children(self.projects.iter().map(|project| {
                let id = project.definition.id.clone();
                let selected = !self.creating && self.selected_id().as_deref() == Some(id.as_str());
                action_button(
                    format!("open-project-{id}"),
                    Intent::Quiet,
                    Some(IconName::LayoutDashboard),
                    project.definition.title.clone(),
                )
                .selected(selected)
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.creating = false;
                    this.open(id.clone(), window, cx);
                }))
            }))
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

    fn project_content(&self, cx: &mut Context<Self>, tokens: &Tokens) -> Div {
        let Some(project) = &self.presentation else {
            return div();
        };
        let running = matches!(
            project.status,
            SessionStatus::Starting | SessionStatus::Running
        );
        let failed = project.status == SessionStatus::Failed;
        let changed = project
            .applied_revision
            .as_ref()
            .is_some_and(|revision| revision != &project.working.revision);
        v_flex().size_full().min_h_0().gap_2()
            .child(h_flex().p_3().gap_3().flex_wrap()
                .child(div().flex_1().min_w_0().child(project.working.definition.title.clone()))
                .child(action_button("project-start", Intent::Neutral, Some(IconName::Play), if failed { "Restart…" } else { "Start…" })
                    .disabled(self.busy || running).on_click(cx.listener(|this, _, window, cx| this.confirm_launch(false, window, cx))))
                .when(running || changed, |header| header.child(action_button("project-apply", Intent::Neutral, Some(IconName::RefreshCw), "Apply & Restart…")
                    .disabled(self.busy).on_click(cx.listener(|this, _, window, cx| this.confirm_launch(true, window, cx)))))
                .child(action_button("project-stop", Intent::Neutral, Some(IconName::Square), "Stop")
                    .disabled(self.busy || !running).on_click(cx.listener(|this, _, window, cx| this.stop(window, cx))))
                .child(self.project_menu(cx))
                .child(action_button("project-close", Intent::Quiet, Some(IconName::X), "Close")
                    .disabled(self.busy).on_click(cx.listener(|this, _, window, cx| this.close(window, cx)))))
            .child(v_flex().px_3().gap_1().text_color(tokens.muted)
                .child(format!("Working · definition {} · source {}", short(&project.working.revision.definition), short(&project.working.revision.source)))
                .child(project.applied_revision.as_ref().map_or_else(|| "Running · none (not started)".into(), |revision| format!("Running · definition {} · source {} · {:?}", short(&revision.definition), short(&revision.source), project.status)))
                .when(changed, |region| region.child("Working changes are not running. Apply & Restart explicitly."))
                .when(failed, |region| region.child("Worker failed. Last valid results remain visible; choose Restart to run again.")))
            .child(div().flex_1().min_h_0().children(self.dashboard.clone()))
            .when(!project.diagnostics.is_empty(), |content| content.child(div().px_3().text_color(tokens.muted).child(project.diagnostics.clone())))
    }

    fn project_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let owner = cx.weak_entity();
        let editors = self.editors.clone();
        let button = action_button(
            "project-menu",
            Intent::Quiet,
            Some(IconName::Ellipsis),
            "Project",
        )
        .disabled(self.busy)
        .dropdown_caret(true);
        anchored::dropdown_menu(button, move |menu, _, _| {
            let settings_owner = owner.clone();
            let export_owner = owner.clone();
            let refresh_owner = owner.clone();
            let mut menu = menu.item(
                PopupMenuItem::new("Settings…")
                    .icon(IconName::Settings)
                    .on_click(move |_, window, cx| {
                        _ = settings_owner.update(cx, |this, cx| this.show_settings(window, cx));
                    }),
            );
            for editor in &editors {
                let editor_owner = owner.clone();
                let editor = editor.clone();
                menu = menu.item(
                    PopupMenuItem::new(format!("Open in {}", editor.title))
                        .icon(IconName::FileCode)
                        .on_click(move |_, window, cx| {
                            _ = editor_owner
                                .update(cx, |this, cx| this.edit(editor.clone(), window, cx));
                        }),
                );
            }
            if editors.is_empty() {
                menu = menu.item(PopupMenuItem::new("VS Code and Zed not detected").disabled(true));
            }
            menu.item(
                PopupMenuItem::new("Refresh working files")
                    .icon(IconName::RefreshCw)
                    .on_click(move |_, window, cx| {
                        _ = refresh_owner.update(cx, |this, cx| {
                            if let Some(id) = this.selected_id() {
                                this.open(id, window, cx);
                            }
                        });
                    }),
            )
            .item(
                PopupMenuItem::new("Export…")
                    .icon(IconName::Download)
                    .on_click(move |_, window, cx| {
                        _ = export_owner.update(cx, |this, cx| this.confirm_export(window, cx));
                    }),
            )
        })
    }
}

fn short(revision: &str) -> String {
    revision.chars().take(12).collect()
}
