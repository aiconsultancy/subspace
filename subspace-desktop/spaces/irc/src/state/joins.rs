//! Join requests that choose a conversation and autojoins retired by a switch.

use subspace_client_irc::{BufferKey, Event};

use super::removed::{Removed, same};

#[derive(Debug, Clone, Default)]
pub(crate) struct Joins {
    requested: Vec<BufferKey>,
    initiative: Option<BufferKey>,
    retired: Removed,
}

impl Joins {
    pub fn request(&mut self, buffer: &BufferKey) {
        if !self.requested.contains(buffer) {
            self.requested.push(buffer.clone());
        }
    }

    pub fn focus(&mut self, buffer: Option<BufferKey>) {
        self.initiative = buffer;
    }

    pub fn show(&mut self, buffer: &BufferKey) -> bool {
        let before = self.requested.len();
        self.requested.retain(|asked| !same(asked, buffer));
        let initiative = self
            .initiative
            .as_ref()
            .is_some_and(|wanted| same(wanted, buffer));
        if initiative {
            self.initiative = None;
        }
        initiative || self.requested.len() != before
    }

    pub fn retired(&self, buffer: &BufferKey) -> bool {
        self.retired.get(buffer).is_some()
    }

    /// Preserve retired buffers in history but hide them until the next join.
    pub fn admit(&mut self, event: &Event) -> bool {
        if let Event::AutojoinRetired(buffer) = event {
            self.retired.insert(buffer, None);
            if self
                .initiative
                .as_ref()
                .is_some_and(|pending| same(pending, buffer))
            {
                self.initiative = None;
            }
            return false;
        }
        self.retired.admit(event)
    }
}
