use super::super::HEADER_ACTION_MAX_SHARE;
use super::{
    Canvas, ChatPanel, Context, Tokens,
    actions::{self, CanvasAction},
    kind_icon,
};
use crate::action::{Intent, icon_button};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, div, relative,
};
use subspace_dashboard::{
    icon_button::{IconButton, Metrics},
    overflow_toolbar::OverflowToolbar,
};
use subspace_lucide::IconName;

impl ChatPanel {
    pub(super) fn canvas_header(
        &self,
        canvas: &Canvas,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let list = &self.tab().canvases;
        let units = actions::units(canvas, list.gallery, cx);
        let toolbar = OverflowToolbar::sync(&mut self.canvas_toolbar.borrow_mut(), units, cx);
        let size = Metrics::current(cx);
        h_flex()
            .id("canvas-header")
            .test_support()
            .flex_shrink_0()
            .flex_nowrap()
            .min_w_0()
            .min_h(size.row_height())
            .whitespace_nowrap()
            .p_1()
            .gap(size.gap())
            .border_b_1()
            .border_color(tokens.line)
            .child(kind_icon(canvas.kind))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(canvas.title.clone()),
            )
            .child(list.position())
            .child(
                div()
                    .flex_shrink_1()
                    .min_w(size.overflow_width())
                    .max_w(relative(HEADER_ACTION_MAX_SHARE))
                    .child(toolbar),
            )
            .into_any_element()
    }

    pub(super) fn canvas_previous(id: &'static str, cx: &mut Context<Self>) -> IconButton {
        icon_button(id, Intent::Quiet, IconName::ChevronLeft, "Previous canvas").on_click(
            cx.listener(|chat, _, window, cx| CanvasAction::Previous.activate(chat, window, cx)),
        )
    }

    pub(super) fn canvas_next(id: &'static str, cx: &mut Context<Self>) -> IconButton {
        icon_button(id, Intent::Quiet, IconName::ChevronRight, "Next canvas").on_click(
            cx.listener(|chat, _, window, cx| CanvasAction::Next.activate(chat, window, cx)),
        )
    }
}
