//! Reconcile loaded provider documents by identity, retaining native selection.
use super::{Session, registration_key};
use gpui_kit::base::TextViewState;
use gpui_kit::{Context, Entity, SharedString};
use std::collections::HashSet;
pub(super) struct Pooled {
    pub(super) state: Entity<TextViewState>,
    raw: SharedString,
}
impl Pooled {
    fn new(
        raw: SharedString,
        existing: Option<Entity<TextViewState>>,
        cx: &mut Context<Session>,
    ) -> Self {
        let source = crate::rich::markdown_guard::prepare(raw.clone());
        let state = existing.unwrap_or_else(|| crate::rich::native_state::create(&source, cx));
        state.update(cx, |state, cx| state.set_text(&source, cx));
        Self { state, raw }
    }
    fn update(&mut self, raw: SharedString, cx: &mut Context<Session>) {
        if self.raw == raw {
            return;
        }
        let source = crate::rich::markdown_guard::prepare(raw.clone());
        self.state
            .update(cx, |state, cx| state.set_text(&source, cx));
        self.raw = raw;
    }
}
impl Session {
    pub(in crate::find_in_page) fn refresh_provider(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let (scope, documents) = (self.provider.snapshot)(cx);
        self.transition(scope, cx);
        self.virtual_content = (self.provider.virtual_content)(cx);
        let keys: HashSet<_> = documents.iter().map(|doc| doc.id.clone()).collect();
        self.prune_provider(&keys, cx);
        for (rank, doc) in documents.into_iter().enumerate() {
            self.document(doc, rank, cx);
        }
        self.schedule(cx);
    }
    fn transition(&mut self, scope: SharedString, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        for entry in self.entries.values() {
            entry.highlight(Vec::new(), cx);
        }
        if !self.scope.is_empty() {
            self.entries.retain(|_, entry| entry.identity.is_some());
        }
        self.cursor.clear();
        self.ranges.clear();
        self.pool.clear();
        self.snapshot.clear();
        self.scope = scope;
    }
    fn prune_provider(&mut self, keys: &HashSet<gpui_kit::ElementId>, cx: &mut Context<Self>) {
        self.pool.retain(|id, _| keys.contains(id));
        if !self.virtual_content {
            return;
        }
        self.entries.retain(|_, entry| {
            let removed = entry
                .identity
                .as_ref()
                .is_none_or(|scope| scope == &self.scope)
                && !keys.contains(&entry.element);
            if removed {
                entry.highlight(Vec::new(), cx);
            }
            !removed
        });
    }
    fn document(
        &mut self,
        doc: crate::find_in_page::Document,
        rank: usize,
        cx: &mut Context<Self>,
    ) {
        let id = doc.id.clone();
        let registered = registration_key(&id, Some(&self.scope));
        let existing = self
            .entries
            .get(&registered)
            .or_else(|| self.entries.get(&registration_key(&id, None)))
            .and_then(|entry| entry.native.rich());
        let pooled = self
            .pool
            .entry(id.clone())
            .or_insert_with(|| Pooled::new(doc.markdown.clone(), existing, cx));
        pooled.update(doc.markdown, cx);
        let state = pooled.state.clone();
        self.entries.remove(&registration_key(&id, None));
        self.rich(&doc.id, &state, Some(self.scope.clone()), cx);
        if let Some(entry) = self.entries.get_mut(&registered) {
            entry.rank = rank;
        }
    }
}
