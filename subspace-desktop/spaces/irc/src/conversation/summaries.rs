//! One issue disclosure per message, retained by its conversation.
#[cfg(test)]
mod tests;
use super::Conversation;
use crate::state::Entry;
use gpui_kit::{App, Context, WeakEntity};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};
use subspace_framework::dashboard::animated_collapsible::{Disclosure, Resize};

#[derive(Default)]
pub(crate) struct Summaries {
    panels: HashMap<String, Panel>,
}
#[derive(Default)]
pub(crate) struct Panel {
    pub(crate) issue: String,
    pub(crate) motion: Disclosure,
}
impl Summaries {
    pub(crate) fn panel(&self, message: &str) -> Option<&Panel> {
        self.panels.get(message)
    }
    fn toggle(&mut self, message: String, issue: &str, cx: &App) {
        let panel = self.panels.entry(message).or_default();
        let open = panel.issue != issue || !panel.motion.is_open();
        panel.issue = issue.to_owned();
        panel.motion.set_open(open, cx);
    }
    fn retain<'a>(&mut self, entries: impl Iterator<Item = &'a Entry>) {
        let held: HashSet<_> = entries.map(identity).collect();
        self.panels.retain(|key, _| held.contains(key));
    }
}

pub(crate) fn identity(entry: &Entry) -> String {
    entry.line.id.as_ref().map_or_else(
        || format!("message:{}", entry.seq),
        |id| format!("msgid:{id}"),
    )
}

impl Conversation {
    pub(super) fn prune_summaries(&mut self, cx: &App) {
        let held = self.hub.read(cx).state.buffer(&self.buffer);
        self.summaries
            .retain(held.into_iter().flat_map(|held| held.entries.iter()));
    }
}

pub(crate) fn toggle(view: &WeakEntity<Conversation>, message: String, issue: &str, cx: &mut App) {
    _ = view.update(cx, |view, cx| {
        view.summaries.toggle(message.clone(), issue, cx);
        remeasure(view, &message, cx);
        cx.notify();
    });
}

pub(crate) fn resize(view: WeakEntity<Conversation>, message: String) -> Resize {
    Rc::new(move |cx| {
        _ = view.update(cx, |view, cx| remeasure(view, &message, cx));
    })
}
fn remeasure(view: &Conversation, message: &str, cx: &mut Context<Conversation>) {
    let at = view
        .hub
        .read(cx)
        .state
        .buffer(&view.buffer)
        .and_then(|held| position(held.entries.iter(), message));
    if let Some(index) = at.and_then(|at| crate::transcript::row_of(view.scroll.rows(), at)) {
        view.scroll.remeasure(&[index], cx);
    }
}

fn position<'a>(mut entries: impl Iterator<Item = &'a Entry>, message: &str) -> Option<usize> {
    if let Some(id) = message.strip_prefix("msgid:") {
        entries.position(|entry| entry.line.id.as_deref() == Some(id))
    } else {
        let seq: i64 = message.strip_prefix("message:")?.parse().ok()?;
        entries.position(|entry| entry.line.id.is_none() && entry.seq == seq)
    }
}

#[cfg(test)]
impl Conversation {
    pub(crate) fn is_following_summary_tail(&self, cx: &App) -> bool {
        self.scroll.state.read(cx).is_following_tail()
    }
}
