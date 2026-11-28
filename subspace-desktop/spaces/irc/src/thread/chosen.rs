//! The thread chosen in each conversation, kept for the session: it
//! survives switching the inspector's tabs and switching conversations, and
//! goes once its root is no longer held. Each is rebuilt only when its
//! buffer's held lines changed, judged by a cheap fingerprint, so a sync
//! that changes nothing in a thread's buffer reads none of its lines.

use std::collections::HashMap;
use std::rc::Rc;

use subspace_client_irc::BufferKey;

use super::model::{Thread, build};
use crate::state::{Buffer, IrcState};

/// The chosen thread of each conversation that has one.
#[derive(Default)]
pub(crate) struct Threads {
    chosen: HashMap<BufferKey, Chosen>,
}

/// One conversation's chosen thread.
struct Chosen {
    /// The root message's ID.
    root: String,
    /// The thread as last built.
    shown: Option<Rc<Thread>>,
    /// The buffer as it was when the thread was last built.
    seen: Option<Fingerprint>,
}

/// What changes whenever a held line arrives, goes, folds or unfolds, or a
/// request ends: enough to tell that a thread must be built again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    len: usize,
    first: Option<i64>,
    last: Option<i64>,
    folded: usize,
    ended: bool,
}

impl Fingerprint {
    fn of(held: &Buffer, root: &str) -> Self {
        Self {
            len: held.entries.len(),
            first: held.entries.front().map(|entry| entry.seq),
            last: held.entries.back().map(|entry| entry.seq),
            folded: held.folded.len(),
            ended: held.requests.ending(root).is_some(),
        }
    }
}

impl Threads {
    /// Chooses message `root` as `buffer`'s thread; built at the next
    /// [`Self::refresh`].
    pub(crate) fn choose(&mut self, buffer: &BufferKey, root: String) {
        self.chosen.insert(
            buffer.clone(),
            Chosen {
                root,
                shown: None,
                seen: None,
            },
        );
    }

    /// The root message ID of `buffer`'s chosen thread.
    #[cfg(test)]
    pub(crate) fn chosen(&self, buffer: &BufferKey) -> Option<&str> {
        self.chosen.get(buffer).map(|chosen| chosen.root.as_str())
    }

    /// `buffer`'s chosen thread as last built.
    pub(crate) fn shown(&self, buffer: &BufferKey) -> Option<Rc<Thread>> {
        self.chosen.get(buffer)?.shown.clone()
    }

    /// Builds each chosen thread whose buffer changed, and forgets those
    /// whose root is no longer held. Returns the forgotten ones, by buffer
    /// and root.
    pub(crate) fn refresh(&mut self, state: &IrcState) -> Vec<(BufferKey, String)> {
        let mut gone = Vec::new();
        self.chosen.retain(|buffer, chosen| {
            let kept = state
                .buffer(buffer)
                .is_some_and(|held| chosen.update(buffer, held));
            if !kept {
                gone.push((buffer.clone(), chosen.root.clone()));
            }
            kept
        });
        gone
    }
}

impl Chosen {
    /// Builds the thread again when `held` changed. False when its root is
    /// no longer held.
    fn update(&mut self, buffer: &BufferKey, held: &Buffer) -> bool {
        let now = Fingerprint::of(held, &self.root);
        if self.seen == Some(now) {
            return self.shown.is_some();
        }
        self.seen = Some(now);
        self.shown = build(buffer, held, &self.root).map(Rc::new);
        self.shown.is_some()
    }
}
