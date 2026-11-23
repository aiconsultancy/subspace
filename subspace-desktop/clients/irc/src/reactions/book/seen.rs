//! The message IDs a reaction book has seen loaded.

use std::collections::{BTreeSet, VecDeque};

use super::{Key, MAX_SEEN_IDS};
use crate::model::BufferKey;

/// Message IDs seen loaded, oldest first, bounded by [`MAX_SEEN_IDS`].
#[derive(Debug, Clone, Default)]
pub(super) struct Seen {
    pub(super) ids: BTreeSet<Key>,
    order: VecDeque<Key>,
}

impl Seen {
    pub(super) fn insert(&mut self, key: Key) {
        if !self.ids.insert(key.clone()) {
            return;
        }
        self.order.push_back(key);
        if self.order.len() > MAX_SEEN_IDS
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
    }

    pub(super) fn forget(&mut self, buffer: &BufferKey) {
        self.ids.retain(|(held, _)| held != buffer);
        self.order.retain(|(held, _)| held != buffer);
    }
}
