//! The cards of the bodies not drawn whole: one held back, with its size
//! and **Show**, and one drawn in parts, with the controls that move
//! between its parts ([`super`]).

use gpui_kit::{AnyElement, Context, IntoElement as _, SharedString};
use subspace_client_fleet::activity::ActivityEntry;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use super::super::parts::{caption, card, part_controls, whole};
use super::{Drawer, Key, Pieced, ask};
use crate::Irc;

/// Shows the body of the item `key`, as **Show** asks.
fn show(board: &mut Irc, key: &Key, cx: &mut Context<Irc>) {
    if let Some(selection) = board.inspector_activity.as_mut() {
        ask(selection, key);
        cx.notify();
    }
}

/// Shows part `piece` of the body of `key` drawn in parts by `drawer`.
fn show_piece(board: &mut Irc, (key, drawer): (&Key, Drawer), piece: usize, cx: &mut Context<Irc>) {
    if let Some(selection) = board.inspector_activity.as_mut() {
        super::show_piece(selection, (key, drawer), piece);
        cx.notify();
    }
}

/// A body drawn in parts by `drawer`: why, the controls that move between
/// its parts, and the part shown. Their IDs start with `id`, which tells
/// apart the bodies one card draws in parts, so each pager moves its own.
pub(in super::super) fn pieced_parts(
    id: &str,
    (pieced, drawer): (&Pieced, Drawer),
    cx: &Context<Irc>,
) -> Vec<AnyElement> {
    let key = pieced.key.clone();
    let go = move |board: &mut Irc, piece: usize, cx: &mut Context<Irc>| {
        show_piece(board, (&key, drawer), piece, cx);
    };
    let piece = &pieced.piece;
    vec![
        caption(
            &format!("{id}:pieced"),
            &format!(
                "Too large to draw whole within the view's memory budget ({} bytes), so it shows one part at a time.",
                pieced.bytes
            ),
            cx,
        ),
        part_controls(
            (id, "piece"),
            (piece.piece, piece.pieces.saturating_sub(1)),
            go,
            cx,
        ),
        whole(&format!("{id}:body"), &piece.text, cx),
    ]
}

/// An item drawn in parts: its heading, then what is drawn above its
/// body whole or in parts alike ([`super::super::item::note`]), then its
/// parts.
pub(in super::super) fn pieced(
    id: &str,
    (entry, pieced): (&ActivityEntry, &Pieced),
    cx: &Context<Irc>,
) -> AnyElement {
    let mut parts: Vec<AnyElement> = super::super::item::note(id, entry, cx)
        .into_iter()
        .collect();
    parts.extend(pieced_parts(id, (pieced, Drawer::Plan), cx));
    card(id, &super::super::item::heading(entry), parts, cx)
}

/// An item whose body is not drawn: its heading alone, fixed words of
/// bounded length (SUB decision `33ccecf6` item 2), then why, its size,
/// and **Show**.
pub(in super::super) fn held_back(
    id: &str,
    (key, entry, bytes): (&Key, &ActivityEntry, u64),
    cx: &Context<Irc>,
) -> AnyElement {
    card(
        id,
        &super::super::item::heading(entry),
        held_back_parts(id, (key, bytes), cx),
        cx,
    )
}

/// Why a body is not drawn, its size, and **Show**.
pub(in super::super) fn held_back_parts(
    id: &str,
    (key, bytes): (&Key, u64),
    cx: &Context<Irc>,
) -> Vec<AnyElement> {
    let key = key.clone();
    let show = action_button(
        SharedString::from(format!("{id}:show")),
        Intent::Neutral,
        Some(IconName::Eye),
        "Show",
    )
    .on_click(cx.listener(move |board, _, _, cx| self::show(board, &key, cx)))
    .into_any_element();
    vec![
        caption(
            &format!("{id}:held-back"),
            &format!("Not drawn, to stay within the view's memory budget ({bytes} bytes)."),
            cx,
        ),
        show,
    ]
}
