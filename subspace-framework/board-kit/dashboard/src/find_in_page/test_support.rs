//! Native fixtures shared only by local-find regressions.
use super::{FindInPage, Provider};
use crate::rich::{self, Format};
use gpui_kit::base::TextSelectionLayer;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window, div,
};
pub(super) struct Content {
    pub(super) focus: FocusHandle,
}
impl Render for Content {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .size_full()
            .child(TextSelectionLayer)
            .child(rich::plain_text("find-title", "Needle title"))
            .child(rich::rich_text(
                "find-comment",
                Format::Markdown,
                "A **needle** in a comment, then NEEDLE.",
            ))
    }
}

impl gpui_kit::Focusable for Content {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
pub(super) fn mount(cx: &mut TestAppContext) -> (Entity<FindInPage>, &mut VisualTestContext) {
    mount_with(cx, |cx| Content {
        focus: cx.focus_handle(),
    })
}
pub(super) fn mount_with<D: Render + gpui_kit::Focusable + 'static>(
    cx: &mut TestAppContext,
    create: impl FnOnce(&mut Context<D>) -> D,
) -> (Entity<FindInPage>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (pane, cx) = cx.add_window_view(|window, cx| {
        let content = cx.new(create);
        content.read(cx).focus_handle(cx).focus(window, cx);
        FindInPage::new(content.into(), Provider::default(), window, cx)
    });
    cx.run_until_parked();
    (pane, cx)
}
