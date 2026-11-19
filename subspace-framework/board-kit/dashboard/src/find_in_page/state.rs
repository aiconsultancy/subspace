//! One retained local-find session per Space.
use super::{
    model::{self, FindCursor},
    participant::{Entry, Snapshot},
    provider::Provider,
};
use gpui_kit::base::TextViewState;
use gpui_kit::{Context, ElementId, Entity, SharedString, Window};
use std::{collections::HashMap, ops::Range, sync::Arc};
mod refresh;
mod registration;
const ACTIVE_OPACITY: f32 = 0.55;
const MATCH_OPACITY: f32 = 0.25;

pub(crate) struct Session {
    pub(super) open: bool,
    query: String,
    pub(super) cursor: FindCursor,
    entries: HashMap<RegistrationKey, Entry>,
    snapshot: Vec<(Arc<str>, gpui_kit::EntityId, Snapshot)>,
    pool: HashMap<ElementId, refresh::Pooled>,
    ranges: HashMap<Arc<str>, Vec<(Range<usize>, bool)>>,
    scope: SharedString,
    virtual_content: bool,
    pending: bool,
    reveal_pending: bool,
    provider: Provider,
}
pub(crate) fn key(id: &ElementId) -> Arc<str> {
    format!("{id:?}").into()
}
pub(super) type RegistrationKey = (Option<SharedString>, ElementId);
fn registration_key(id: &ElementId, identity: Option<&SharedString>) -> RegistrationKey {
    (identity.cloned(), id.clone())
}
impl Session {
    pub(super) fn new(provider: Provider) -> Self {
        Self {
            open: false,
            query: String::new(),
            cursor: FindCursor::default(),
            entries: HashMap::new(),
            snapshot: Vec::new(),
            pool: HashMap::new(),
            ranges: HashMap::new(),
            scope: SharedString::default(),
            virtual_content: false,
            pending: false,
            reveal_pending: false,
            provider,
        }
    }
    pub(crate) fn highlights(
        &self,
        id: &ElementId,
        identity: Option<&SharedString>,
        cx: &gpui_kit::App,
    ) -> Vec<(Range<usize>, gpui_kit::Hsla)> {
        if identity.is_some_and(|scope| scope != &self.scope) {
            return Vec::new();
        }
        let identity = identity
            .cloned()
            .or_else(|| self.virtual_content.then(|| self.scope.clone()));
        self.entries
            .get(&registration_key(id, identity.as_ref()))
            .map_or_else(Vec::new, |entry| self.colors(&entry.id, cx))
    }
    fn colors(&self, id: &Arc<str>, cx: &gpui_kit::App) -> Vec<(Range<usize>, gpui_kit::Hsla)> {
        let accent = crate::Tokens::current(cx).accent;
        self.ranges
            .get(id)
            .into_iter()
            .flatten()
            .map(|(range, active)| {
                (
                    range.clone(),
                    accent.opacity(if *active {
                        ACTIVE_OPACITY
                    } else {
                        MATCH_OPACITY
                    }),
                )
            })
            .collect()
    }
    pub(crate) fn pooled(
        &self,
        id: &ElementId,
        identity: Option<&SharedString>,
    ) -> Option<Entity<TextViewState>> {
        if identity.is_some_and(|scope| scope != &self.scope) {
            return None;
        }
        self.pool.get(id).map(|pooled| pooled.state.clone())
    }
    pub(super) fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.pending || !self.open {
            return;
        }
        self.pending = true;
        let me = cx.entity().downgrade();
        cx.defer(move |cx| {
            _ = me.update(cx, |this, cx| {
                this.pending = false;
                this.rebuild(true, cx);
            });
        });
    }
    pub(super) fn show(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        self.refresh_provider(cx);
        self.reveal_pending = true;
        self.rebuild(false, cx);
    }
    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.snapshot.clear();
        self.cursor.clear();
        self.ranges.clear();
        self.apply(cx);
        self.pool.clear();
        self.reveal_pending = false;
        cx.notify();
    }
    pub(super) fn query(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;
        self.reveal_pending = true;
        self.rebuild(false, cx);
    }
    fn active_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .values()
            .filter(|entry| {
                entry
                    .identity
                    .as_ref()
                    .is_none_or(|scope| scope == &self.scope)
            })
            .filter(|entry| !self.virtual_content || self.pool.contains_key(&entry.element))
    }
    fn rebuild(&mut self, retain: bool, cx: &mut Context<Self>) {
        self.entries.retain(|_, entry| entry.native.alive());
        let mut entries: Vec<_> = self.active_entries().collect();
        entries.sort_by(|a, b| super::participant::compare(a, b, self.virtual_content));
        let snapshot: Vec<_> = entries
            .into_iter()
            .filter_map(|entry| {
                entry
                    .native
                    .snapshot(cx)
                    .map(|text| (entry.id.clone(), entry.native.id(), text))
            })
            .collect();
        if retain && snapshot == self.snapshot {
            return;
        }
        self.snapshot = snapshot;
        let docs = self
            .snapshot
            .iter()
            .map(|(id, _, text)| (id.as_ref(), text.as_str()));
        self.cursor
            .replace(model::find(&self.query, docs).unwrap_or_default(), retain);
        self.group_ranges();
        self.apply(cx);
        cx.notify();
    }
    fn group_ranges(&mut self) {
        self.ranges.clear();
        for hit in self.cursor.hits() {
            self.ranges
                .entry(hit.document.clone())
                .or_default()
                .push((hit.range.clone(), self.cursor.active() == Some(hit)));
        }
    }
    pub(super) fn step(&mut self, backwards: bool, _: &mut Window, cx: &mut Context<Self>) {
        self.cursor.step(backwards);
        self.reveal_pending = true;
        self.group_ranges();
        self.apply(cx);
        cx.notify();
    }
    pub(super) fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.reveal_pending) {
            return;
        }
        let Some(hit) = self.cursor.active() else {
            return;
        };
        (self.provider.reveal)(&hit.document, window, cx);
        if let Some(entry) = self.active_entries().find(|entry| entry.id == hit.document) {
            entry.native.reveal(hit.range.clone(), cx);
        }
    }
    fn apply(&self, cx: &mut Context<Self>) {
        for entry in self.active_entries() {
            entry.highlight(self.colors(&entry.id, cx), cx);
        }
    }
}
#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
