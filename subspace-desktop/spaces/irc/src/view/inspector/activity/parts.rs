//! The pieces items are drawn from: a labelled card, a muted caption and
//! a block of untrusted text, whole, wrapped and selectable.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::rich::plain_text;

use super::text::visible;
use crate::Irc;

/// An item: its label above its parts.
pub(super) fn card(id: &str, label: &str, parts: Vec<AnyElement>, cx: &Context<Irc>) -> AnyElement {
    let tokens = Tokens::current(cx);
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .pl(tokens.indent)
        .child(
            div()
                .text_size(tokens.secondary_size())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(plain_text(SharedString::from(format!("{id}:label")), label)),
        )
        .children(parts)
        .into_any_element()
}

/// A muted line of the view's own words or a short value.
pub(super) fn caption(id: &str, words: &str, cx: &Context<Irc>) -> AnyElement {
    let tokens = Tokens::current(cx);
    let shown = visible(words);
    div()
        .id(SharedString::from(id.to_owned()))
        .test_support()
        .w_full()
        .min_w_0()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(plain_text(SharedString::from(format!("{id}:text")), &shown).test_text(shown))
        .into_any_element()
}

/// A block under its own small label.
pub(super) fn labelled(id: &str, label: &str, text: &str, cx: &Context<Irc>) -> AnyElement {
    let tokens = Tokens::current(cx);
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            div()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(label.to_owned()),
        )
        .child(block(id, text, cx))
        .into_any_element()
}

/// Untrusted text whole, in the monospace font, wrapped to the inspector's
/// width and selectable.
pub(super) fn block(id: &str, text: &str, cx: &Context<Irc>) -> AnyElement {
    let tokens = Tokens::current(cx);
    let shown = visible(text);
    div()
        .id(SharedString::from(id.to_owned()))
        .test_support()
        .w_full()
        .min_w_0()
        .p_2()
        .rounded(tokens.control_radius)
        .bg(tokens.surface)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(tokens.mono_font)
        .text_color(tokens.fg)
        .child(plain_text(SharedString::from(format!("{id}:text")), &shown).test_text(shown))
        .into_any_element()
}
