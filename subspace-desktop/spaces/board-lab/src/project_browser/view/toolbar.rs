//! The pane's toolbar: the open project's name on the leading side and the
//! actions for it on the trailing side. While the navigator is hidden, a menu
//! at the leading end offers what the navigator holds; the navigator's switch
//! is the status bar's.

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::{
    App, Context, Div, FontWeight, IntoElement, ParentElement as _, Styled as _, WeakEntity,
};
use subspace_framework::dashboard::action::{Intent, action_button, main_action};
use subspace_framework::dashboard::anchored::{self, Dropdown};
use subspace_framework::dashboard::icon_button::{IconButton, Metrics};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tokens::ButtonRole;
use subspace_framework::dashboard::workspace::Toolbar;
use subspace_framework::dashboard::{Tokens, truncated};

use super::super::ProjectBrowser;
use super::super::facts::{Opened, WorkerFacts};
use super::{on_show, worker_facts};
use crate::shell::{self, ViewTab};

/// A project in the view menu: its ID, its title and whether it is open.
struct ProjectChoice {
    id: String,
    title: String,
    open: bool,
}

impl ProjectBrowser {
    pub(super) fn toolbar(&self, opened: &Opened, cx: &mut Context<Self>) -> Toolbar {
        let toolbar = Toolbar::new("board-lab-toolbar");
        let toolbar = match self.shell.navigator {
            true => toolbar,
            false => toolbar.leading(self.view_menu(cx)),
        };
        let toolbar = toolbar.leading(title(opened.toolbar_title(), cx));
        match (opened, &self.presentation) {
            (Opened::Project(_), Some(project)) => {
                self.project_actions(toolbar, worker_facts(project), cx)
            }
            (Opened::Welcome, _) => toolbar.trailing(self.new_project_action(cx)),
            _ => toolbar,
        }
    }

    /// New project sits in the toolbar while nothing is open, as the Space's
    /// main action: the toolbar's one primary button.
    fn new_project_action(&self, cx: &mut Context<Self>) -> IconButton {
        main_action("project-new-toolbar", IconName::Plus, shell::NEW_PROJECT)
            .disabled(self.busy)
            .on_click(cx.listener(|this, _, window, cx| this.begin_creating(window, cx)))
    }

    /// The open project's actions, in the order its header had them: Start
    /// or Restart, Apply & Restart when it applies, Stop, the Project menu
    /// and Close.
    fn project_actions(
        &self,
        toolbar: Toolbar,
        worker: WorkerFacts,
        cx: &mut Context<Self>,
    ) -> Toolbar {
        let start = toolbar_action("project-start", IconName::Play, worker.start_label())
            .disabled(self.busy || worker.running)
            .on_click(cx.listener(|this, _, window, cx| this.confirm_launch(false, window, cx)));
        let stop = toolbar_action("project-stop", IconName::Square, "Stop")
            .disabled(self.busy || !worker.running)
            .on_click(cx.listener(|this, _, window, cx| this.stop(window, cx)));
        let close = action_button("project-close", Intent::Quiet, Some(IconName::X), "Close")
            .role(ButtonRole::Toolbar)
            .disabled(self.busy)
            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx)));
        let toolbar = toolbar.trailing(start);
        let toolbar = if worker.offers_apply() {
            toolbar.trailing(self.apply_action(cx))
        } else {
            toolbar
        };
        toolbar
            .trailing(stop)
            .trailing(self.project_menu(cx))
            .trailing(close)
    }

    fn apply_action(&self, cx: &mut Context<Self>) -> IconButton {
        toolbar_action("project-apply", IconName::RefreshCw, "Apply & Restart…")
            .disabled(self.busy)
            .on_click(cx.listener(|this, _, window, cx| this.confirm_launch(true, window, cx)))
    }

    /// Settings, the detected editors, Refresh working files and Export.
    fn project_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let owner = cx.weak_entity();
        let editors = self.editors.clone();
        let button = action_button(
            "project-menu",
            Intent::Quiet,
            Some(IconName::Ellipsis),
            "Project",
        )
        .role(ButtonRole::Toolbar)
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

    /// What the navigator holds, as a menu for while it is hidden: the
    /// views, the projects, New project and Refresh projects.
    fn view_menu(&self, cx: &mut Context<Self>) -> Dropdown {
        let owner = cx.weak_entity();
        let on_show = on_show(cx);
        let views = self.shell.views;
        let choices = self.project_choices();
        let busy = self.busy;
        let button = shell::view_menu_button(ViewTab::Projects);
        anchored::dropdown_menu(button, move |menu, _, _| {
            let menu = shell::view_items(menu, views, ViewTab::Projects, &on_show).separator();
            let menu = project_items(menu, &choices, busy, &owner).separator();
            navigator_commands(menu, busy, &owner)
        })
    }

    fn project_choices(&self) -> Vec<ProjectChoice> {
        let open = (!self.creating).then(|| self.selected_id()).flatten();
        self.projects
            .iter()
            .map(|project| ProjectChoice {
                id: project.definition.id.clone(),
                title: project.definition.title.clone(),
                open: open.as_deref() == Some(project.definition.id.as_str()),
            })
            .collect()
    }
}

/// The toolbar's leading name, on the centre line of its buttons.
fn title(text: &str, cx: &App) -> Div {
    let tokens = Tokens::current(cx);
    h_flex()
        .h(Metrics::current(cx).target())
        .min_w_0()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.fg_strong)
        .child(truncated::text("board-lab-toolbar-title", text.to_owned()))
}

/// A labelled action with the toolbar's geometry.
fn toolbar_action(id: &'static str, icon: IconName, label: &'static str) -> IconButton {
    action_button(id, Intent::Neutral, Some(icon), label).role(ButtonRole::Toolbar)
}

/// The projects in the view menu, the open one checked.
fn project_items(
    mut menu: PopupMenu,
    choices: &[ProjectChoice],
    busy: bool,
    owner: &WeakEntity<ProjectBrowser>,
) -> PopupMenu {
    if choices.is_empty() {
        return menu.item(PopupMenuItem::new("No projects yet").disabled(true));
    }
    for choice in choices {
        let owner = owner.clone();
        let id = choice.id.clone();
        menu = menu.item(
            PopupMenuItem::new(choice.title.clone())
                .icon(IconName::LayoutDashboard)
                .checked(choice.open)
                .disabled(busy)
                .on_click(move |_, window, cx| {
                    _ = owner.update(cx, |this, cx| this.open_listed(id.clone(), window, cx));
                }),
        );
    }
    menu
}

/// The navigator's header and footer actions in the view menu.
fn navigator_commands(
    menu: PopupMenu,
    busy: bool,
    owner: &WeakEntity<ProjectBrowser>,
) -> PopupMenu {
    let create = owner.clone();
    let refresh = owner.clone();
    menu.item(
        PopupMenuItem::new(shell::NEW_PROJECT)
            .icon(IconName::Plus)
            .disabled(busy)
            .on_click(move |_, window, cx| {
                _ = create.update(cx, |this, cx| this.begin_creating(window, cx));
            }),
    )
    .item(
        PopupMenuItem::new("Refresh projects")
            .icon(IconName::RefreshCw)
            .disabled(busy)
            .on_click(move |_, window, cx| {
                _ = refresh.update(cx, |this, cx| this.refresh_catalog(window, cx));
            }),
    )
}
