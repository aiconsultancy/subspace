//! An item too large to send inline: what it is and a load action, then
//! the item whole once read.

use gpui_kit::{AnyElement, Context, IntoElement as _, SharedString};
use subspace_client_fleet::activity::PagedItem;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use super::parts::{caption, card};
use super::text::visible;
use super::{Loaded, Selection};
use crate::Irc;

/// An item too large to send inline: what it is, then a load action, or
/// the item in full once read.
pub(super) fn paged(
    selection: &Selection,
    turn: &str,
    paged: &PagedItem,
    cx: &Context<Irc>,
) -> AnyElement {
    let id = format!("irc-activity:{turn}:{}", paged.base.id);
    let kind = match paged.kind.as_str() {
        "tool" => "Tool",
        "text" => "Reply",
        "reasoning" => "Reasoning",
        "step" => "Step",
        _ => "Item",
    };
    let name = paged.name.as_deref().map(|name| format!(" · {name}"));
    let label = visible(&format!(
        "{kind}{} · {} · {} bytes in {} parts",
        name.unwrap_or_default(),
        paged.status,
        paged.paged.bytes,
        paged.paged.pages
    ));
    let key = (turn.to_owned(), paged.base.id.clone());
    let loaded = selection.loaded.get(&key);
    let parts = load_parts(&id, &key, paged, loaded, cx);
    card(&id, &label, parts, cx)
}

pub(super) fn load_parts(
    id: &str,
    key: &(String, String),
    paged: &PagedItem,
    loaded: Option<&Loaded>,
    cx: &Context<Irc>,
) -> Vec<AnyElement> {
    let action = |label: &'static str| {
        let (turn, item) = key.clone();
        action_button(
            SharedString::from(format!("{id}:load")),
            Intent::Neutral,
            Some(IconName::Download),
            label,
        )
        .on_click(cx.listener(move |board, _, _, cx| super::load(board, &turn, &item, cx)))
        .into_any_element()
    };
    match loaded.map(|loaded| loaded.result.as_ref()) {
        None => vec![action("Load in full")],
        Some(None) => vec![caption(&format!("{id}:loading"), "Loading…", cx)],
        Some(Some(Ok(item))) if item.base().seq < paged.base.seq => {
            vec![action("Load the latest"), super::item::item(id, item, cx)]
        }
        Some(Some(Ok(item))) => vec![super::item::item(id, item, cx)],
        Some(Some(Err(error))) => vec![
            caption(&format!("{id}:failed"), &error.to_string(), cx),
            action("Try again"),
        ],
    }
}
