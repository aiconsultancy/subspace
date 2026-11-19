//! Native participant registration, release and structural ordering.
use super::{Session, registration_key};
use crate::find_in_page::{
    participant::{Entry, Native},
    state::RegistrationKey,
};
use crate::rich::plain::PlainState;
use gpui_kit::base::TextViewState;
use gpui_kit::{Context, ElementId, Entity, SharedString};
impl Session {
    pub(crate) fn rich(
        &mut self,
        id: &ElementId,
        state: &Entity<TextViewState>,
        identity: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.register(id, Native::Rich(state.downgrade()), identity, cx);
    }
    pub(crate) fn plain(
        &mut self,
        id: &ElementId,
        state: &Entity<PlainState>,
        identity: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.register(id, Native::Plain(state.downgrade()), identity, cx);
    }
    fn register(
        &mut self,
        id: &ElementId,
        native: Native,
        identity: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let identity = identity.or_else(|| self.virtual_content.then(|| self.scope.clone()));
        if self.open && self.virtual_content && !self.pool.contains_key(id) {
            return;
        }
        let registered = registration_key(id, identity.as_ref());
        if self
            .entries
            .get(&registered)
            .is_some_and(|entry| entry.native.id() == native.id())
        {
            return;
        }
        let rank = self
            .entries
            .get(&registered)
            .map_or(usize::MAX, |entry| entry.rank);
        if let Some(mut entry) = Entry::new(id.clone(), registered.clone(), identity, native, cx) {
            entry.rank = rank;
            self.entries.insert(registered, entry);
        }
        self.schedule(cx);
    }
    pub(in crate::find_in_page) fn released(
        &mut self,
        id: &RegistrationKey,
        native: gpui_kit::EntityId,
        cx: &mut Context<Self>,
    ) {
        if self
            .entries
            .get(id)
            .is_some_and(|entry| entry.native.id() == native)
        {
            self.entries.remove(id);
            self.schedule(cx);
        }
    }
    pub(crate) fn order_changed(
        &self,
        id: &ElementId,
        identity: Option<&SharedString>,
        rank: usize,
    ) -> bool {
        !self.virtual_content
            && self
                .entries
                .get(&registration_key(id, identity))
                .is_some_and(|entry| entry.rank != rank)
    }
    pub(crate) fn order(
        &mut self,
        id: &ElementId,
        identity: Option<&SharedString>,
        rank: usize,
        cx: &mut Context<Self>,
    ) {
        if self.virtual_content {
            return;
        }
        if let Some(entry) = self.entries.get_mut(&registration_key(id, identity)) {
            if entry.rank == rank {
                return;
            }
            entry.rank = rank;
            self.schedule(cx);
        }
    }
}
