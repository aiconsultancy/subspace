//! Last-request usage presentation on the existing shared anchor host.

use super::ChatToolbar;
use super::labels::usage_line;
use super::menus::button;
use crate::anchored::Anchor;
use crate::tokens::Tokens;
use gpui_kit::component::{ThemeStyled as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div, px,
};
use subspace_lucide::IconName;

/// The widest the usage line may run before it wraps.
const USAGE_WIDTH: f32 = 260.;

impl ChatToolbar {
    pub(super) fn usage(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.anchors.trigger(
            "chat-usage",
            button("chat-usage", IconName::Gauge, "Usage")
                .on_click(cx.listener(|toolbar, _, window, cx| toolbar.toggle_usage(window, cx))),
            cx,
        )
    }

    // qual:allow(iosp) reason: "The existing repeat-click transaction must read this anchor's open state, then either dismiss it or dismiss other popups and show usage. Splitting that event boundary would hide its ordering."
    fn toggle_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.anchors.anchor("chat-usage", cx);
        if host.read(cx).is_open() {
            host.update(cx, |anchor, cx| anchor.dismiss(window, cx));
            return;
        }
        self.dismiss_popups(window, cx);
        Self::show_usage(host, window, cx);
    }

    pub(super) fn show_usage(host: Entity<Anchor>, window: &mut Window, cx: &mut Context<Self>) {
        let toolbar = cx.entity();
        let body = cx.new(|cx| UsageBody {
            toolbar: toolbar.downgrade(),
            focus: cx.focus_handle(),
            _subscription: cx.observe(&toolbar, |_, _, cx| cx.notify()),
        });
        host.update(cx, |anchor, cx| anchor.show(body, window, cx));
    }
}

struct UsageBody {
    toolbar: WeakEntity<ChatToolbar>,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl Focusable for UsageBody {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for UsageBody {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let line = self.toolbar.upgrade().map(|toolbar| {
            let model = toolbar.read(cx).model();
            usage_line(&model.usage, model.show_ai_units)
        });
        div()
            .id("chat-usage-body")
            .test_support()
            .track_focus(&self.focus)
            .popover_style(cx)
            .p_3()
            .child(
                v_flex()
                    .gap_1()
                    .max_w(px(USAGE_WIDTH))
                    .child(div().text_color(tokens.fg_strong).child("Last request"))
                    .child(div().text_color(tokens.muted).children(line)),
            )
    }
}
