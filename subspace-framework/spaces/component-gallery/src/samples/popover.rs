//! A fixture body for popovers hosted by `anchored::Anchor`, the shared
//! boundary for every Subspace popup.

use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::anchored::{self, Anchor};
use subspace_dashboard::icon_button::IconButton;

const BODY_WIDTH: f32 = 240.;

/// A retained anchor and the body it shows.
pub(crate) struct PopoverHost {
    pub(crate) anchor: Entity<Anchor>,
    body: Entity<Body>,
}

impl PopoverHost {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            anchor: cx.new(Anchor::new),
            body: cx.new(|cx| Body {
                focus: cx.focus_handle(),
            }),
        }
    }

    /// The trigger that opens the body, with the anchor beside it.
    pub(crate) fn trigger(&self, button: IconButton) -> impl IntoElement {
        let anchor = self.anchor.clone();
        let body = self.body.clone();
        let button = button.on_click(move |_, window, cx| {
            anchor.update(cx, |anchor, cx| anchor.show(body.clone(), window, cx));
        });
        div()
            .child(anchored::trigger(&self.anchor, button))
            .child(self.anchor.clone())
    }

    pub(crate) fn dismiss(&self, window: &mut Window, cx: &mut App) {
        self.anchor
            .update(cx, |anchor, cx| anchor.dismiss(window, cx));
    }
}

/// Fixture popover content; Escape or an outside press closes it.
pub(crate) struct Body {
    focus: FocusHandle,
}

impl Focusable for Body {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Body {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex()
            .track_focus(&self.focus)
            .w(gpui_kit::px(BODY_WIDTH))
            .gap_1()
            .p(tokens.pad_x)
            .child(div().text_color(tokens.fg_strong).child("Fixture popover"))
            .child(
                div()
                    .text_size(tokens.secondary_size())
                    .text_color(tokens.muted)
                    .child("Contextual content that does not interrupt the task. Press Escape to close."),
            )
    }
}
