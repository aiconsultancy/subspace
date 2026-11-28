//! The navigator's header strip: the Space's name, then any badge and header
//! actions. Settings › Layout can hide the name; the strip then stays only
//! for a badge or actions, which have no other place.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, IntoElement, ParentElement as _, SharedString, Styled as _,
    TestSupportExt as _, Window, div,
};

use super::{header, strip_height};
use crate::tokens::Tokens;
use crate::typography::heading_font;

/// What the header holds.
pub(super) struct Head {
    pub(super) title: SharedString,
    pub(super) badge: Option<SharedString>,
    pub(super) actions: Vec<AnyElement>,
    pub(super) show_title: bool,
}

/// The header strip, or none when the title is hidden and there is no badge
/// or action to hold.
pub(super) fn head(head: Head, window: &Window, cx: &App) -> Option<impl IntoElement> {
    if !head.show_title && head.badge.is_none() && head.actions.is_empty() {
        return None;
    }
    let tokens = Tokens::current(cx);
    let strip = header::strip(
        "workspace-navigator-header",
        tokens.sidebar_line,
        strip_height(window, cx),
        &tokens,
    );
    Some(
        // Without the title, a spacer keeps the badge and actions on the
        // trailing edge, where the title leaves them.
        strip
            .map(|strip| {
                if head.show_title {
                    strip.child(title(head.title, &tokens, cx))
                } else {
                    strip.child(div().flex_1())
                }
            })
            .children(head.badge.map(|badge| badge_mark(badge, &tokens)))
            .children(head.actions)
            .test_support(),
    )
}

/// The Space's name, in the accent so it reads as where you are.
fn title(title: SharedString, tokens: &Tokens, cx: &App) -> impl IntoElement {
    header::title(
        "workspace-navigator-title",
        title,
        FontWeight::BOLD,
        heading_font(cx),
        tokens,
    )
    .text_color(tokens.accent)
    .test_support()
}

fn badge_mark(badge: SharedString, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .child(badge)
}
