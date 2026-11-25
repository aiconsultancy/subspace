//! The pieces items are drawn from: a labelled card, a muted caption and
//! a block of untrusted text, whole, wrapped and selectable, a text over
//! [`LONG`] in parts of [`PART`]. Their menus offer Copy and Select all,
//! never Add to chat: officer activity stays out of the chat.

use std::collections::HashMap;

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::rich::{PlainText, plain_text_with};
use subspace_framework::dashboard::row_menu::MenuChat;

use super::text::visible;
use crate::Irc;

/// A text longer than this shows in parts.
const LONG: usize = 256 * 1024;
/// The most bytes of a text one part shows.
const PART: usize = 64 * 1024;

/// The part shown of each long text, by its block's ID; the first when
/// none is.
pub(super) type Shown = HashMap<String, usize>;

/// Selectable text whose menu leaves out the chat.
fn selectable(id: impl Into<SharedString>, value: &str) -> PlainText {
    let id: SharedString = id.into();
    plain_text_with(id, value, MenuChat::Off)
}

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
                .child(selectable(format!("{id}:label"), label)),
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
        .child(selectable(format!("{id}:text"), &shown).test_text(shown))
        .into_any_element()
}

/// A block under its own small label.
pub(super) fn labelled(
    id: &str,
    label: &str,
    text: &str,
    shown: &Shown,
    cx: &Context<Irc>,
) -> AnyElement {
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
        .child(block(id, text, shown, cx))
        .into_any_element()
}

/// Untrusted text whole, in the monospace font, wrapped to the inspector's
/// width and selectable; a text over [`LONG`] one part at a time, under
/// the controls that move between its parts.
pub(super) fn block(id: &str, text: &str, shown: &Shown, cx: &Context<Irc>) -> AnyElement {
    if text.len() <= LONG {
        return whole(id, text, cx);
    }
    let parts = split(text);
    let last = parts.len() - 1;
    let part = shown.get(id).copied().unwrap_or(0).min(last);
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(part_controls(id, part, last, cx))
        .child(whole(id, parts[part], cx))
        .into_any_element()
}

/// `text` cut into parts of at most [`PART`] bytes, each ending on a
/// character boundary, so every byte is in exactly one part.
pub(super) fn split(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = PART.min(rest.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (part, after) = rest.split_at(end);
        parts.push(part);
        rest = after;
    }
    parts
}

/// First, Previous, Next and Last, and which part shows.
fn part_controls(id: &str, part: usize, last: usize, cx: &Context<Irc>) -> AnyElement {
    let moves = [
        ("first", "First part", IconName::ChevronsLeft, 0),
        (
            "previous",
            "Previous part",
            IconName::ChevronLeft,
            part.saturating_sub(1),
        ),
        (
            "next",
            "Next part",
            IconName::ChevronRight,
            (part + 1).min(last),
        ),
        ("last", "Last part", IconName::ChevronsRight, last),
    ];
    let buttons = moves.into_iter().map(|(name, label, icon, to)| {
        let block = id.to_owned();
        action_button(
            SharedString::from(format!("{id}:part-{name}")),
            Intent::Quiet,
            Some(icon),
            label,
        )
        .on_click(cx.listener(move |board, _, _, cx| super::show_part(board, &block, to, cx)))
        .into_any_element()
    });
    h_flex()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap_1()
        .children(buttons)
        .child(caption(
            &format!("{id}:part"),
            &format!("Part {} of {}", part + 1, last + 1),
            cx,
        ))
        .into_any_element()
}

/// Untrusted text whole, in the monospace font.
fn whole(id: &str, text: &str, cx: &Context<Irc>) -> AnyElement {
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
        .child(selectable(format!("{id}:text"), &shown).test_text(shown))
        .into_any_element()
}
