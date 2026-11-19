//! Top-level render composition for [`super::Workbench`]: the tab bar and
//! the per-tab subviews (canvas, gallery, environments).

mod canvas;
mod controls;
mod dims;
mod environments;
mod gallery;
mod result;

use gpui_kit::component::h_flex;
use gpui_kit::component::v_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabSpec};

use super::Workbench;
use super::state::ViewTab;

const PROTOTYPE_TABS: [ViewTab; 4] = [
    ViewTab::Projects,
    ViewTab::Canvas,
    ViewTab::Gallery,
    ViewTab::Environments,
];
const DEFAULT_TABS: [ViewTab; 2] = [ViewTab::Projects, ViewTab::Environments];

fn view_tab_order(prototypes: bool) -> &'static [ViewTab] {
    if prototypes {
        &PROTOTYPE_TABS
    } else {
        &DEFAULT_TABS
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Tokens::current(cx);
        v_flex()
            .size_full()
            .min_h_0()
            .bg(t.bg)
            .text_color(t.fg)
            .text_size(t.row_font)
            .track_focus(&self.focus)
            .child(view_tabs(self, cx, &t))
            .child(match self.view.tab.visible(self.prototype_views) {
                ViewTab::Projects => div()
                    .flex_1()
                    .min_h_0()
                    .child(self.projects.clone())
                    .into_any_element(),
                ViewTab::Canvas => canvas::canvas(self, window, cx, &t).into_any_element(),
                ViewTab::Gallery => gallery::gallery(self, window, cx, &t).into_any_element(),
                ViewTab::Environments => v_flex()
                    .id("workbench-environments-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(environments::environment_panel(self, window, cx, &t))
                    .into_any_element(),
            })
    }
}

fn view_tabs(this: &Workbench, cx: &mut Context<Workbench>, t: &Tokens) -> impl IntoElement {
    let tabs = view_tab_order(this.prototype_views);
    h_flex()
        .px(t.pad_x)
        .border_b_1()
        .border_color(t.line)
        .justify_between()
        .child(
            dashboard_tabs::tab_bar("workbench-tabs")
                .selected_index(
                    tabs.iter()
                        .position(|tab| *tab == this.view.tab.visible(this.prototype_views))
                        .unwrap_or_default(),
                )
                .children(tabs.iter().map(|tab| match tab {
                    ViewTab::Projects => {
                        dashboard_tabs::tab(TabSpec::new("Projects", IconName::FolderOpen))
                    }
                    ViewTab::Canvas => {
                        dashboard_tabs::tab(TabSpec::new("Canvas", IconName::FlaskConical))
                    }
                    ViewTab::Gallery => {
                        dashboard_tabs::tab(TabSpec::new("Dashboard", IconName::LayoutDashboard))
                    }
                    ViewTab::Environments => {
                        dashboard_tabs::tab(TabSpec::new("Environments", IconName::Cpu))
                    }
                }))
                .on_click(cx.listener(|this, index: &usize, window, cx| {
                    this.view.tab = view_tab_order(this.prototype_views)
                        .get(*index)
                        .copied()
                        .unwrap_or(ViewTab::Environments);
                    this.projects.update(cx, |projects, cx| {
                        if this.view.tab == ViewTab::Projects {
                            projects.activate(window, cx);
                        } else {
                            projects.deactivate();
                        }
                    });
                    cx.notify();
                })),
        )
        .child(
            div()
                .text_size(t.caption_size())
                .text_color(t.muted)
                .child("Alpha"),
        )
}

#[cfg(test)]
mod tests {
    use super::{ViewTab, view_tab_order};
    use crate::{BoardLab, PROTOTYPE_VIEWS};
    use subspace_framework::core::config::Config;
    use subspace_framework::dashboard::Dashboard;

    #[test]
    fn prototype_tabs_are_hidden_by_default() {
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
    fn registered_opt_in_shows_prototype_tabs() {
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
}
