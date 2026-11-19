use super::{Disclosure, Resize, state::Sample};
use gpui_kit::base::ElementExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div,
};

#[derive(IntoElement)]
pub(super) struct Frame {
    state: Disclosure,
    content: AnyElement,
    resize: Option<Resize>,
    preview: Option<f32>,
}
impl Frame {
    pub(super) fn new(
        state: Disclosure,
        content: AnyElement,
        resize: Option<Resize>,
        preview: Option<f32>,
    ) -> Self {
        Self {
            state,
            content,
            resize,
            preview,
        }
    }
    fn presentation(&self, window: &mut Window, cx: &mut App) -> Sample {
        let animating = self.state.animating() && !cx.reduce_motion();
        let sample = self.preview.map_or_else(
            || self.state.sample(cx),
            |progress| self.state.preview(progress),
        );
        // The next-frame callback owns intermediate reports. A real clock can
        // advance between that callback and render, even past the endpoint.
        if !animating && self.state.report_height(sample.height) {
            resized(self.resize.clone(), cx);
        }
        if self.preview.is_none() && animating {
            next_frame(self.state.clone(), self.resize.clone(), window);
        }
        sample
    }
}
impl RenderOnce for Frame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let sample = self.presentation(window, cx);
        if self.preview.is_none() && !self.state.is_visible(cx) {
            return div().h_0().into_any_element();
        }
        let settled_open =
            self.preview.is_none() && self.state.is_open() && !self.state.animating();
        let state = self.state;
        let resize = self.resize;
        // Animate only the outer height. Settled content uses normal flow so
        // a width change takes its natural height on the very first frame.
        div()
            .relative()
            .w_full()
            .min_w_0()
            .when(!settled_open, |frame| frame.h(sample.height))
            .overflow_hidden()
            .child(
                div()
                    .when(!settled_open, |content| content.absolute().top_0().left_0())
                    .w_full()
                    .opacity(sample.opacity)
                    .on_prepaint(move |bounds, window, cx| {
                        if state.measure(bounds.size.height, cx) {
                            next_frame(state.clone(), resize.clone(), window);
                        }
                    })
                    .child(self.content),
            )
            .into_any_element()
    }
}
fn resized(resize: Option<Resize>, cx: &mut App) {
    if let Some(resize) = resize {
        // MessageScroller holds its list borrow during row layout.
        cx.defer(move |cx| resize(cx));
    }
}

fn next_frame(state: Disclosure, resize: Option<Resize>, window: &mut Window) {
    // Invalidate BEFORE the list measures this frame, preserving its tail anchor.
    window.on_next_frame(move |window, cx| {
        if state.report_height(state.sample(cx).height)
            && let Some(resize) = resize
        {
            resize(cx);
        }
        window.refresh();
    });
}
