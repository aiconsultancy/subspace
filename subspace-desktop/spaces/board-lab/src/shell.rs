//! What Board Lab's views share of the workspace shell: the views the
//! navigator switches between, whether the navigator shows, and the navigator
//! header, view switch and view menu that every view draws the same way.
//!
//! The Workbench owns the facts. The project browser draws from a copy and
//! asks for changes with a [`ShellRequest`]. The navigator's switch is the
//! status bar's, which reaches the Workbench through `Dashboard::toggle_dock`.

use std::rc::Rc;

use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, Div, ParentElement as _, Styled as _, Window};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, icon_button, menu_button};
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabSpec};
use subspace_framework::dashboard::workspace::{self, Navigator};

/// The Space's name, which is the navigator's title.
const SPACE_TITLE: &str = "Board Lab";
/// The Space's stage, shown beside its name.
const STAGE: &str = "Alpha";
/// The label of every New project control.
pub(crate) const NEW_PROJECT: &str = "New project";
/// Whether the navigator shows, stored with the Space's other choices.
pub(crate) const NAVIGATOR_KEY: &str = "board-lab.navigator";
const DOCK_SHOWN: &str = "1";
const DOCK_HIDDEN: &str = "0";

const PROTOTYPE_TABS: [ViewTab; 4] = [
    ViewTab::Projects,
    ViewTab::Canvas,
    ViewTab::Gallery,
    ViewTab::Environments,
];
const DEFAULT_TABS: [ViewTab; 2] = [ViewTab::Projects, ViewTab::Environments];

/// A view of the Space: native projects, the retained Canvas and Dashboard
/// prototypes (draft authoring and the saved-preset gallery), or the
/// environments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewTab {
    Projects,
    Canvas,
    Gallery,
    Environments,
}

impl ViewTab {
    pub(crate) fn visible(self, prototypes: bool) -> Self {
        match (self, prototypes) {
            (Self::Canvas | Self::Gallery, false) => Self::Projects,
            _ => self,
        }
    }

    /// The view's name in the switch, in the view menu and on its pane tab.
    pub(crate) fn label(self) -> &'static str {
        self.words().0
    }

    /// The view's glyph in the view menu and on its pane tab.
    pub(crate) fn icon(self) -> IconName {
        self.words().1
    }

    /// The view's name and glyph, in one place per variant.
    fn words(self) -> (&'static str, IconName) {
        match self {
            Self::Projects => ("Projects", IconName::FolderOpen),
            Self::Canvas => ("Canvas", IconName::FlaskConical),
            Self::Gallery => ("Dashboard", IconName::LayoutDashboard),
            Self::Environments => ("Environments", IconName::Cpu),
        }
    }
}

/// The views the switch offers, in order. The prototypes show only when the
/// person opted in.
pub(crate) fn view_tab_order(prototypes: bool) -> &'static [ViewTab] {
    if prototypes {
        &PROTOTYPE_TABS
    } else {
        &DEFAULT_TABS
    }
}

/// The view at `index` in the switch; past the end, Environments.
pub(crate) fn view_at(views: &[ViewTab], index: usize) -> ViewTab {
    views.get(index).copied().unwrap_or(ViewTab::Environments)
}

/// Whether a dock shows, from its stored choice. It does unless the person
/// hid it.
pub(crate) fn dock_shown(stored: Option<&str>) -> bool {
    stored != Some(DOCK_HIDDEN)
}

/// The stored choice for a dock that shows or is hidden.
pub(crate) fn dock_value(shown: bool) -> &'static str {
    if shown { DOCK_SHOWN } else { DOCK_HIDDEN }
}

/// The shell facts every view draws from: the views the switch offers and
/// whether the navigator shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shell {
    pub(crate) views: &'static [ViewTab],
    pub(crate) navigator: bool,
}

impl Default for Shell {
    /// A view on its own: the default views, with the navigator showing.
    fn default() -> Self {
        Self {
            views: view_tab_order(false),
            navigator: true,
        }
    }
}

/// What a view asks the owner of the shell facts to change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellRequest {
    Show(ViewTab),
}

/// Shows a view, from the switch or the view menu.
pub(crate) type OnShow = Rc<dyn Fn(ViewTab, &mut Window, &mut App)>;

/// The navigator every view starts from: the Space's name and stage in its
/// header, then the view switch. The caller adds its header action, its list
/// and its footer.
pub(crate) fn navigator(shell: Shell, current: ViewTab, on_show: OnShow, cx: &App) -> Navigator {
    Navigator::new(SPACE_TITLE)
        .badge(STAGE)
        .child(view_switch(shell.views, current, on_show, cx))
}

/// The switch between the Space's views, at the navigator's text edge.
fn view_switch(views: &'static [ViewTab], current: ViewTab, on_show: OnShow, cx: &App) -> Div {
    let tokens = Tokens::current(cx);
    let selected = views
        .iter()
        .position(|view| *view == current)
        .unwrap_or_default();
    // Text-only tabs, so the default pair fits the navigator's default width.
    let tabs = dashboard_tabs::tab_bar("workbench-tabs")
        .selected_index(selected)
        .children(
            views
                .iter()
                .map(|view| dashboard_tabs::tab(TabSpec::plain(view.label()))),
        )
        .on_click(move |index: &usize, window: &mut Window, cx: &mut App| {
            on_show(view_at(views, *index), window, cx);
        });
    h_flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .px(workspace::text_edge(&tokens))
        .py_2()
        .child(tabs)
}

/// The navigator's New project action. The caller adds its state and
/// handler.
pub(crate) fn new_project_action() -> IconButton {
    icon_button("project-new", Intent::Quiet, IconName::Plus, NEW_PROJECT)
}

/// The trigger of the view menu, which stands in for the navigator while it
/// is hidden. It names the current view.
pub(crate) fn view_menu_button(current: ViewTab) -> IconButton {
    menu_button(
        "board-lab-view-menu",
        current.icon(),
        current.label(),
        false,
    )
}

/// Adds the Space's views to the view menu, the current one checked.
pub(crate) fn view_items(
    mut menu: PopupMenu,
    views: &'static [ViewTab],
    current: ViewTab,
    on_show: &OnShow,
) -> PopupMenu {
    for view in views.iter().copied() {
        let on_show = on_show.clone();
        menu = menu.item(
            PopupMenuItem::new(view.label())
                .icon(view.icon())
                .checked(view == current)
                .on_click(move |_, window, cx| on_show(view, window, cx)),
        );
    }
    menu
}

#[cfg(test)]
mod tests {
    use subspace_framework::core::config::Config;
    use subspace_framework::dashboard::Dashboard;

    use super::{Shell, ViewTab, dock_shown, dock_value, view_at, view_tab_order};
    use crate::{BoardLab, PROTOTYPE_VIEWS};

    #[test]
    fn prototype_views_are_hidden_by_default() {
        let config = Config::default();
        assert_eq!(
            view_tab_order(config.feature_enabled(&PROTOTYPE_VIEWS)),
            &[ViewTab::Projects, ViewTab::Environments]
        );
        for tab in [ViewTab::Canvas, ViewTab::Gallery] {
            assert_eq!(
                tab.visible(config.feature_enabled(&PROTOTYPE_VIEWS)),
                ViewTab::Projects
            );
        }
    }

    #[test]
    fn registered_opt_in_shows_prototype_views() {
        let mut config = Config::default();
        let settings = BoardLab::settings();
        assert_eq!(settings.features, &[PROTOTYPE_VIEWS]);
        assert_eq!(PROTOTYPE_VIEWS.id, "board.board-lab.prototype-views");
        assert_eq!(PROTOTYPE_VIEWS.title, "Show prototype views");
        assert_eq!(
            PROTOTYPE_VIEWS.stage,
            subspace_framework::core::features::FeatureStage::Experimental
        );
        assert!(!settings.features[0].default_enabled && settings.features[0].toggleable);
        config.features.insert(PROTOTYPE_VIEWS.id.into(), true);
        let enabled = config.feature_enabled(&PROTOTYPE_VIEWS);
        assert_eq!(
            view_tab_order(enabled),
            &[
                ViewTab::Projects,
                ViewTab::Canvas,
                ViewTab::Gallery,
                ViewTab::Environments
            ]
        );
        assert_eq!(ViewTab::Gallery.visible(enabled), ViewTab::Gallery);
    }

    #[test]
    fn the_switch_names_each_view_once() {
        let labels = view_tab_order(true)
            .iter()
            .map(|view| view.label())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["Projects", "Canvas", "Dashboard", "Environments"]);
    }

    #[test]
    fn the_switch_resolves_an_index_to_its_view() {
        let views = view_tab_order(false);
        assert_eq!(view_at(views, 0), ViewTab::Projects);
        assert_eq!(view_at(views, 1), ViewTab::Environments);
        assert_eq!(view_at(views, 7), ViewTab::Environments, "past the end");
        assert_eq!(view_at(view_tab_order(true), 2), ViewTab::Gallery);
    }

    #[test]
    fn a_dock_shows_until_it_is_hidden() {
        assert!(dock_shown(None), "shown before any choice is stored");
        assert!(dock_shown(Some("")), "an unknown value does not hide it");
        assert!(dock_shown(Some(dock_value(true))));
        assert!(!dock_shown(Some(dock_value(false))));
    }

    #[test]
    fn a_view_on_its_own_shows_the_default_views_and_the_navigator() {
        let shell = Shell::default();
        assert_eq!(shell.views, view_tab_order(false));
        assert!(shell.navigator);
    }
}
