//! Horizontal and resizable vertical workspace composition.

use gpui_kit::base::{h_resizable, resizable_panel};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement, Stateful,
    Styled, Window, div, px,
};
use subspace_dashboard::Tokens;

use super::super::SubspaceApp;
use super::super::navigation::{self, NavigationMode};

pub(super) fn root(_window: &Window, _cx: &App) -> Stateful<Div> {
    let shell = v_flex().id("subspace");
    #[cfg(any(test, all(debug_assertions, feature = "capture")))]
    let shell = crate::capture::root(shell, _window, _cx);
    shell
}

impl SubspaceApp {
    pub(super) fn workspace_split(
        &mut self,
        tokens: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.navigation_mode != NavigationMode::Vertical {
            return h_flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .items_stretch()
                .child(self.workspace_body(window, cx))
                .into_any_element();
        }
        self.vertical_workspace(tokens, window, cx)
    }

    fn vertical_workspace(
        &mut self,
        tokens: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cap = self.rail_content_max(window, cx);
        let min = navigation::effective_collapsed_rail_width(cx);
        let max = cap.max(min);
        let shown = self.shown_rail_width(window, cx);
        self.queue_rail_restore(cap, shown, window, cx);
        let app = cx.weak_entity();
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                h_resizable("workspace-split")
                    .with_state(&self.rail_state)
                    .on_resize(move |state, window, cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.finish_rail_resize(state, window, cx);
                        });
                    })
                    .child(
                        resizable_panel()
                            .size(px(shown))
                            .size_range(px(min)..px(max))
                            .flex_none()
                            .child(self.vertical_navigation(tokens, cx)),
                    )
                    .child(resizable_panel().child(self.workspace_body(window, cx))),
            )
            .into_any_element()
    }
}
