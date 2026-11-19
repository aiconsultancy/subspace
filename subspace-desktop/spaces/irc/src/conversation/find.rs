//! Loaded transcript documents and native row reveal for local find.
use super::{Conversation, mentions};
use gpui_kit::App;
pub(crate) fn documents(
    view: &Conversation,
    cx: &App,
) -> Vec<(usize, subspace_framework::dashboard::find_in_page::Document)> {
    let hub = view.hub.read(cx);
    let Some(held) = hub.state.buffer(&view.buffer) else {
        return Vec::new();
    };
    view.find_cache.borrow_mut().documents(
        view.scroll.rows(),
        held,
        (&mentions(view, cx), Some(hub)),
        cx,
    )
}
pub(crate) fn reveal(view: &Conversation, id: &str, cx: &mut App) {
    let row = view.find_cache.borrow().indices.get(id).copied();
    if let Some(row) = row {
        view.scroll.state.update(cx, |state, cx| {
            state.scroll_to_item(row, cx);
        });
    }
}
mod cache;
pub(crate) use cache::Cache;

pub(crate) fn identity(view: &Conversation) -> gpui_kit::SharedString {
    crate::uploads::viewer::find_scope(view)
        .unwrap_or_else(|| format!("{:?}:{:?}", view.hub.entity_id(), view.buffer).into())
}
pub(crate) fn scope(
    view: &Conversation,
    content: gpui_kit::AnyElement,
    cx: &App,
) -> gpui_kit::AnyElement {
    subspace_framework::dashboard::find_in_page::identified(
        identity(view),
        view.find_active,
        content,
        cx,
    )
}

#[derive(PartialEq, Eq)]
pub(super) struct Revision {
    ends: (Option<i64>, Option<i64>, usize),
    context: (u64, u64),
}
pub(super) fn revision(view: &Conversation, cx: &App) -> Revision {
    use std::hash::{Hash as _, Hasher as _};
    let hub = view.hub.read(cx);
    let mut nicks = std::collections::hash_map::DefaultHasher::new();
    let ends = hub
        .state
        .buffer(&view.buffer)
        .map_or(Default::default(), |held| {
            for member in &held.members {
                member.nick.hash(&mut nicks);
            }
            (
                held.entries.front().map(|e| e.seq),
                held.entries.back().map(|e| e.seq),
                held.entries.len(),
            )
        });
    Revision {
        ends,
        context: (nicks.finish(), hub.issues.recognition_revision()),
    }
}
