//! The workspace shell around the Environments view and the retained
//! prototypes: the shared navigator, a tab that names the view, the toolbar
//! and the view itself. The project browser draws the same shell around
//! Projects.

use std::rc::Rc;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{AnyElement, App, Context, IntoElement as _, ParentElement as _, Window};
use subspace_framework::dashboard::anchored::{self, Dropdown};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabBar, TabSpec};
use subspace_framework::dashboard::workspace::{NavRow, Navigator, Pane, Toolbar, Workspace};

use super::super::Workbench;
use super::super::state::ViewTab;
use crate::shell::{self, OnShow};

/// The readiness words an environment card uses, for its navigator row.
fn readiness(ready: bool) -> &'static str {
    if ready { "ready" } else { "not provisioned" }
}

/// `content` is the view's own element; it owns its scrolling.
pub(super) fn workspace(
    this: &Workbench,
    view: ViewTab,
    content: AnyElement,
    cx: &mut Context<Workbench>,
) -> AnyElement {
    let pane = Pane::new("board-lab-pane")
        .tabs(view_tab(view))
        .toolbar(toolbar(this, view, cx))
        .child(content);
    let workspace = Workspace::new("board-lab-workspace").pane(pane);
    match this.view.navigator {
        true => workspace
            .navigator(navigator(this, view, cx))
            .into_any_element(),
        false => workspace.into_any_element(),
    }
}

/// The pane's one tab, which names the view that is open in it.
fn view_tab(view: ViewTab) -> TabBar {
    dashboard_tabs::pane_tab_bar("board-lab-open")
        .selected_index(0)
        .child(dashboard_tabs::tab(TabSpec::new(view.label(), view.icon())))
}

fn navigator(this: &Workbench, view: ViewTab, cx: &mut Context<Workbench>) -> Navigator {
    let on_show = on_show(cx);
    let creating_blocked = this.projects.read(cx).busy();
    let navigator = shell::navigator(this.shell(), view, on_show, cx).action(
        shell::new_project_action()
            .disabled(creating_blocked)
            .on_click(cx.listener(|this, _, window, cx| this.new_project(window, cx))),
    );
    if view == ViewTab::Environments {
        navigator.children(environment_rows(this, cx))
    } else {
        navigator
    }
}

/// One row for each configured environment. A row selects its environment,
/// as its card in the pane does.
fn environment_rows(this: &Workbench, cx: &mut Context<Workbench>) -> Vec<NavRow> {
    let selected = this
        .selected_env
        .as_deref()
        .unwrap_or(this.draft.environment.as_str());
    this.catalog
        .iter()
        .map(|info| {
            let id = info.spec.id.clone();
            let version = info.spec.version.clone();
            NavRow::new(format!("workbench-nav-env-{id}"), info.spec.title.clone())
                .icon(IconName::Cpu)
                .count(readiness(info.ready))
                .selected(id == selected)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_environment(id.clone(), version.clone(), window, cx);
                }))
        })
        .collect()
}

/// The toolbar holds, while the navigator is hidden, the menu that stands
/// in for it; the navigator's switch is the status bar's. These views keep
/// their own controls in their content.
fn toolbar(this: &Workbench, view: ViewTab, cx: &mut Context<Workbench>) -> Toolbar {
    let toolbar = Toolbar::new("board-lab-toolbar");
    match this.view.navigator {
        true => toolbar,
        false => toolbar.leading(view_menu(this, view, cx)),
    }
}

/// The navigator's choices as a menu: the views and New project. The
/// environments stay selectable on their cards.
fn view_menu(this: &Workbench, view: ViewTab, cx: &mut Context<Workbench>) -> Dropdown {
    let owner = cx.weak_entity();
    let on_show = on_show(cx);
    let views = this.shell().views;
    let creating_blocked = this.projects.read(cx).busy();
    anchored::dropdown_menu(shell::view_menu_button(view), move |menu, _, _| {
        let owner = owner.clone();
        shell::view_items(menu, views, view, &on_show)
            .separator()
            .item(
                PopupMenuItem::new(shell::NEW_PROJECT)
                    .icon(IconName::Plus)
                    .disabled(creating_blocked)
                    .on_click(move |_, window, cx| {
                        _ = owner.update(cx, |this, cx| this.new_project(window, cx));
                    }),
            )
    })
}

fn on_show(cx: &mut Context<Workbench>) -> OnShow {
    let owner = cx.weak_entity();
    Rc::new(move |view: ViewTab, window: &mut Window, cx: &mut App| {
        _ = owner.update(cx, |this, cx| this.show_view(view, window, cx));
    })
}

impl Workbench {
    /// Selects an environment as its card does: the selection, and its
    /// version in the pin field.
    fn select_environment(
        &mut self,
        id: String,
        version: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_env = Some(id);
        self.inputs
            .version_input
            .update(cx, |input, cx| input.set_value(version, window, cx));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::readiness;

    #[test]
    fn a_navigator_row_uses_the_environment_cards_words() {
        assert_eq!(readiness(true), "ready");
        assert_eq!(readiness(false), "not provisioned");
    }
}
