//! The tab strip owns feedback; Kit owns the drag payload and layout edits.

use crate::{Tokens, tabs::Tab};
use gpui_kit::component::dock::{BasePanelView, DragPanel, PanelHandle};
use gpui_kit::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px, size,
};
use std::sync::Arc;

// Match Kit's native preview geometry so the base drop placeholder starts here.
const PREVIEW_WIDTH: f32 = 96.;
const PREVIEW_HEIGHT: f32 = 30.;
const PREVIEW_OPACITY: f32 = 0.75;

pub(super) fn wire(tab: Tab, drag: DragPanel, panel: Arc<dyn BasePanelView>) -> Tab {
    tab.on_drag(drag, move |drag, offset, _, cx| {
        cx.stop_propagation();
        drag.set_drag_offset(offset);
        drag.set_preview_size(size(px(PREVIEW_WIDTH), px(PREVIEW_HEIGHT)));
        cx.new(|_| Preview {
            panel: panel.clone(),
        })
    })
}

struct Preview {
    panel: Arc<dyn BasePanelView>,
}
impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let title = match PanelHandle::of(&self.panel) {
            Some(handle) => handle.title(window, cx),
            None => gpui_kit::SharedString::from(self.panel.panel_name(cx)).into_any_element(),
        };
        div()
            .id("dock-tab-preview")
            .test_support()
            .w(px(PREVIEW_WIDTH))
            .h(px(PREVIEW_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .border_1()
            .border_color(tokens.hair)
            .rounded(tokens.control_radius)
            .text_color(tokens.fg_strong)
            .bg(tokens.surface)
            .opacity(PREVIEW_OPACITY)
            .child(title)
    }
}
