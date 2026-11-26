//! An item too large to send inline: what it is and a load action, then
//! the item whole once read, or, when it is larger than the view can hold,
//! its JSON one page at a time, every page in reach.

use gpui_kit::{AnyElement, Context, IntoElement as _, SharedString};
use subspace_client_fleet::activity::{ActivityEntry, PagedItem};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use super::blocks::page_block;
use super::draw::Pieced;
use super::parts::{Draw, body_block, caption, card, part_controls};
use super::text::visible;
use super::{Held, Loaded, Parts, Selection};
use crate::Irc;

/// An item too large to send inline: what it is, then a load action, or
/// the item in full once read.
pub(super) fn paged(
    selection: &Selection,
    (turn, paged, pieced): (&str, &PagedItem, Option<&Pieced>),
    draw: &Draw<'_>,
    cx: &Context<Irc>,
) -> AnyElement {
    let id = format!("irc-activity:{turn}:{}", paged.base.id);
    let key = (turn.to_owned(), paged.base.id.clone());
    let loaded = selection.loaded.get(&key);
    // Read whole only when the item and its drawn copy both fit the room.
    let whole = super::load::fits_whole(selection, paged.paged.bytes);
    let shown = Drawing { draw, pieced };
    let parts = load_parts(&id, (&key, paged, whole), loaded, &shown, cx);
    card(&id, &label(paged), parts, cx)
}

/// How an item read is drawn: its blocks, or in parts when it is the body
/// last asked for and too large for the room ([`super::draw`]).
pub(super) struct Drawing<'a> {
    draw: &'a Draw<'a>,
    pieced: Option<&'a Pieced>,
}

/// `Tool · read · completed · 140000 bytes in 3 pages`.
pub(super) fn label(paged: &PagedItem) -> String {
    let kind = match paged.kind.as_str() {
        "tool" => "Tool",
        "text" => "Reply",
        "reasoning" => "Reasoning",
        "step" => "Step",
        _ => "Item",
    };
    let name = paged.name.as_deref().map(|name| format!(" · {name}"));
    visible(&format!(
        "{kind}{} · {} · {} bytes in {} pages",
        name.unwrap_or_default(),
        paged.status,
        paged.paged.bytes,
        paged.paged.pages
    ))
}

/// A button that reads the item again from its first page.
fn load_action(
    id: &str,
    (key, bytes): (&(String, String), u64),
    label: &'static str,
    cx: &Context<Irc>,
) -> AnyElement {
    let (turn, item) = key.clone();
    action_button(
        SharedString::from(format!("{id}:load")),
        Intent::Neutral,
        Some(IconName::Download),
        label,
    )
    .on_click(cx.listener(move |board, _, _, cx| {
        super::load(board, &turn, &item, bytes, cx);
    }))
    .into_any_element()
}

/// The load action, or the item once read. An item let go to make room
/// for others shows its load action again: **Load in full** when it and
/// its drawn copy both fit the room left (`fits`), **Load in parts** when
/// they do not.
pub(super) fn load_parts(
    id: &str,
    (key, paged, fits): (&(String, String), &PagedItem, bool),
    loaded: Option<&Loaded>,
    shown: &Drawing<'_>,
    cx: &Context<Irc>,
) -> Vec<AnyElement> {
    let action = |label| load_action(id, (key, paged.paged.bytes), label, cx);
    let Some(loaded) = loaded else {
        return vec![action(load_label(fits))];
    };
    match &loaded.held {
        Held::Loading => vec![caption(&format!("{id}:loading"), "Loading…", cx)],
        Held::Whole(entry) => {
            let latest = (entry.base().seq < paged.base.seq).then(|| action("Load the latest"));
            let mut drawn: Vec<AnyElement> = latest.into_iter().collect();
            match shown.pieced {
                Some(pieced) => drawn.extend(super::draw::pieced_parts(id, pieced, cx)),
                None => drawn.push(whole(id, entry, shown.draw, cx)),
            }
            drawn
        }
        Held::Parts(parts) => {
            let mut shown_parts = Vec::new();
            if parts.page.seq < paged.base.seq {
                shown_parts.push(action("Load the latest"));
            }
            shown_parts.extend(in_parts(id, key, parts, shown.draw, cx));
            shown_parts
        }
        Held::Failed(error) => vec![
            caption(&format!("{id}:failed"), &error.to_string(), cx),
            action("Try again"),
        ],
    }
}

/// The load action's label: how the item will be read.
pub(super) fn load_label(whole: bool) -> &'static str {
    if whole {
        "Load in full"
    } else {
        "Load in parts"
    }
}

/// An item read whole: drawn as its kind, or as its JSON when this version
/// does not know its kind or shape.
fn whole(id: &str, entry: &ActivityEntry, draw: &Draw<'_>, cx: &Context<Irc>) -> AnyElement {
    match entry {
        // A page read never decodes as a paged item.
        ActivityEntry::Paged(_) => caption(&format!("{id}:failed"), "Invalid response", cx),
        entry => super::item::whole(id, entry, draw, cx),
    }
}

/// An item larger than the view can hold: why it shows in parts, a note
/// when it changed while read, the controls, and the part shown, as the
/// JSON it carries. A part is one page: at most 64 KiB.
fn in_parts(
    id: &str,
    key: &(String, String),
    parts: &Parts,
    draw: &Draw<'_>,
    cx: &Context<Irc>,
) -> Vec<AnyElement> {
    let page = &parts.page;
    let mut shown = vec![caption(
        &format!("{id}:parts"),
        &format!(
            "Too large to hold whole ({} bytes), so its JSON shows one part at a time.",
            page.bytes
        ),
        cx,
    )];
    if parts.newer {
        shown.push(caption(
            &format!("{id}:newer"),
            "A newer version is on the gateway, so this shows it from its first part.",
            cx,
        ));
    }
    let last = page.pages.saturating_sub(1) as usize;
    shown.push(part_controls(
        (id, "page"),
        (page.page as usize, last),
        go(key),
        cx,
    ));
    if let Some(next) = parts.loading {
        shown.push(caption(
            &format!("{id}:page-loading"),
            &format!("Loading part {}…", u64::from(next) + 1),
            cx,
        ));
    }
    if let Some((failed, error)) = &parts.failed {
        let (failed, key) = (*failed, key.clone());
        shown.push(caption(
            &format!("{id}:page-failed"),
            &error.to_string(),
            cx,
        ));
        shown.push(
            action_button(
                SharedString::from(format!("{id}:page-retry")),
                Intent::Neutral,
                Some(IconName::Download),
                "Try again",
            )
            .on_click(cx.listener(move |board, _, _, cx| {
                super::show_page(board, &key, failed, cx);
            }))
            .into_any_element(),
        );
    }
    // One page: at most 64 KiB, drawn in parts of its own when its
    // markers draw more.
    shown.push(body_block(id, &page_block(&page.data), draw, cx));
    shown
}

/// Moves to a page of the item `key`.
fn go(key: &(String, String)) -> impl Fn(&mut Irc, usize, &mut Context<Irc>) + Clone + 'static {
    let key = key.clone();
    move |board, page, cx| {
        let page = u32::try_from(page).unwrap_or(u32::MAX);
        super::show_page(board, &key, page, cx);
    }
}
