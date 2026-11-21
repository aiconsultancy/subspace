//! Top-level render composition for [`super::Workbench`]: the project
//! browser's own workspace for Projects, and the shared workspace frame
//! around the other views (canvas, gallery, environments).

mod canvas;
mod controls;
mod dims;
mod environments;
mod frame;
mod gallery;
mod result;

use gpui_kit::component::v_flex;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use subspace_framework::dashboard::Tokens;

use super::Workbench;
use super::state::ViewTab;

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Tokens::current(cx);
        let view = self.view.tab.visible(self.prototype_views);
        // The project browser draws its own workspace: its navigator lists
        // the projects and its toolbar acts on the open one.
        let body = match view {
            ViewTab::Projects => self.projects.clone().into_any_element(),
            ViewTab::Canvas => {
                let content = canvas::canvas(self, window, cx, &t).into_any_element();
                frame::workspace(self, view, content, cx)
            }
            ViewTab::Gallery => {
                let content = gallery::gallery(self, window, cx, &t).into_any_element();
                frame::workspace(self, view, content, cx)
            }
            ViewTab::Environments => {
                let content = v_flex()
                    .id("workbench-environments-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(environments::environment_panel(self, window, cx, &t))
                    .into_any_element();
                frame::workspace(self, view, content, cx)
            }
        };
        v_flex()
            .size_full()
            .min_h_0()
            .bg(t.bg)
            .text_color(t.fg)
            .text_size(t.row_font)
            .track_focus(&self.focus)
            .child(div().flex_1().min_h_0().child(body))
    }
}
