//! The pieces items are drawn from: a labelled card, a muted caption and
//! a block of untrusted text, whole, wrapped and selectable, a text
//! drawing more than [`LONG`] bytes in parts of at most [`PART`]. Their
//! menus offer Copy and Select all, never Add to chat: officer activity
//! stays out of the chat.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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

use super::blocks::{Block, Source, formatted};
use super::size::count_measure;
use super::text::{unit_len, visible, visible_len};
use crate::Irc;

/// A text drawing more bytes than this, markers and all, shows in parts.
pub(super) const LONG: usize = 256 * 1024;
/// The most bytes one part of a text draws.
pub(super) const PART: usize = 64 * 1024;

/// The part shown of each long text, by its block's ID; the first when
/// none is.
pub(super) type Shown = HashMap<String, usize>;

/// Selectable text whose menu leaves out the chat.
fn selectable(id: impl Into<SharedString>, value: &str) -> PlainText {
    let id: SharedString = id.into();
    plain_text_with(id, value, MenuChat::Off)
}

/// An item: its heading, fixed words of this version's own and bounded
/// ([`super::item::heading`]), above its parts.
pub(super) fn card(id: &str, label: &str, parts: Vec<AnyElement>, cx: &Context<Irc>) -> AnyElement {
    let tokens = Tokens::current(cx);
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .pl(tokens.indent)
        .child(
            div()
                .id(SharedString::from(format!("{id}:heading")))
                .test_support()
                .text_size(tokens.secondary_size())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(selectable(format!("{id}:label"), label).test_text(label.to_owned())),
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

/// A text drawn in parts once it passes its block's limit: where its parts
/// start, and the text a value was formatted into. Kept between frames
/// for the item while its body is drawn whole ([`Draw`]), and counted in
/// what that body holds ([`super::size::body_bytes`]).
pub(super) struct Prepared {
    /// The indented JSON or error text a block draws; `None` for text
    /// drawn as it is.
    text: Option<String>,
    /// Where each part starts, by byte; empty for a text drawn whole.
    cuts: Vec<usize>,
}

/// The blocks prepared for one item, by element ID.
pub(super) type Prepare = RefCell<HashMap<String, Rc<Prepared>>>;

/// What drawing an item's blocks reads: the part shown of each long text,
/// and what is prepared of each block, filled the first time it is drawn.
pub(super) struct Draw<'a> {
    pub(super) shown: &'a Shown,
    pub(super) prepared: &'a Prepare,
}

/// A block of an item's body, under its small label when it has one.
pub(super) fn body_block(
    item: &str,
    block: &Block<'_>,
    draw: &Draw<'_>,
    cx: &Context<Irc>,
) -> AnyElement {
    let id = format!("{item}:{}", block.suffix);
    let drawn = self::block(&id, block, draw, cx);
    let Some(label) = block.label else {
        return drawn;
    };
    let tokens = Tokens::current(cx);
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            div()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(label),
        )
        .child(drawn)
        .into_any_element()
}

/// Untrusted text whole, in the monospace font, wrapped to the inspector's
/// width and selectable; a text drawing more than its block's limit one
/// part at a time, under the controls that move between its parts. Only
/// the part shown is drawn.
fn block(id: &str, block: &Block<'_>, draw: &Draw<'_>, cx: &Context<Irc>) -> AnyElement {
    let prepared = prepared(id, block, draw.prepared);
    let text = source_text(prepared.text.as_deref(), block.source);
    let Some(last) = prepared.cuts.len().checked_sub(1) else {
        return whole(id, text, cx);
    };
    let part = draw.shown.get(id).copied().unwrap_or(0).min(last);
    let end = prepared.cuts.get(part + 1).copied().unwrap_or(text.len());
    let start = prepared.cuts[part];
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(part_controls((id, "part"), (part, last), show_part(id), cx))
        .child(whole(id, &text[start..end], cx))
        .into_any_element()
}

/// What is prepared of the block `id`: from `cache` once it was drawn.
fn prepared(id: &str, block: &Block<'_>, cache: &Prepare) -> Rc<Prepared> {
    if let Some(prepared) = cache.borrow().get(id) {
        return prepared.clone();
    }
    let prepared = Rc::new(prepare(block));
    cache.borrow_mut().insert(id.to_owned(), prepared.clone());
    prepared
}

/// Formats a block's value, if it has one, and cuts it into parts when it
/// draws more than the block's limit.
fn prepare(block: &Block<'_>) -> Prepared {
    count_measure();
    let text = formatted(block.source);
    let drawn = source_text(text.as_deref(), block.source);
    let cuts = if visible_len(drawn) > block.long {
        split(drawn)
    } else {
        Vec::new()
    };
    Prepared { text, cuts }
}

/// What a block draws: the text formatted for it, or its own.
fn source_text<'a>(formatted: Option<&'a str>, source: Source<'a>) -> &'a str {
    match (formatted, source) {
        (Some(text), _) => text,
        (None, Source::Text(text)) => text,
        (None, _) => "",
    }
}

/// Where each part of `text` starts: parts drawing at most [`PART`]
/// bytes, markers and all ([`super::text::unit_len`]), each ending on a
/// character boundary, so every character is in exactly one part.
pub(super) fn split(text: &str) -> Vec<usize> {
    let mut cuts = vec![0];
    let mut len = 0;
    for (at, ch) in text.char_indices() {
        let unit = unit_len(ch);
        if len > 0 && len + unit > PART {
            cuts.push(at);
            len = 0;
        }
        len += unit;
    }
    cuts
}

/// First, Previous, Next and Last, and which part shows: `go` moves to a
/// part, by its index. Their IDs are `{id}:{kind}-first` and so on, the caption's
/// `{id}:{kind}`.
pub(super) fn part_controls(
    (id, kind): (&str, &str),
    (part, last): (usize, usize),
    go: impl Fn(&mut Irc, usize, &mut Context<Irc>) + Clone + 'static,
    cx: &Context<Irc>,
) -> AnyElement {
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
        let go = go.clone();
        action_button(
            SharedString::from(format!("{id}:{kind}-{name}")),
            Intent::Quiet,
            Some(icon),
            label,
        )
        .on_click(cx.listener(move |board, _, _, cx| go(board, to, cx)))
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
            &format!("{id}:{kind}"),
            &format!("Part {} of {}", part + 1, last + 1),
            cx,
        ))
        .into_any_element()
}

/// Shows a part of the long text in the block `id`.
fn show_part(id: &str) -> impl Fn(&mut Irc, usize, &mut Context<Irc>) + Clone + 'static {
    let block = id.to_owned();
    move |board, part, cx| super::show_part(board, &block, part, cx)
}

/// Untrusted text whole, in the monospace font.
pub(super) fn whole(id: &str, text: &str, cx: &Context<Irc>) -> AnyElement {
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
