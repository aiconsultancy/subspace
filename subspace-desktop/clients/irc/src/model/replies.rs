//! Reply relationships remain independent of delivery order and presentation.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use chrono::{DateTime, Utc};

use super::{BufferKey, Command, Line, LineKind, Target};

#[cfg(test)]
mod tests;

/// A borrowed parent index for one buffer's loaded lines. Build once when
/// resolving many replies; rebuild after loading or removing history.
pub struct ThreadRoots<'a> {
    buffer: &'a BufferKey,
    parents: BTreeMap<&'a str, &'a Line>,
    resolved: RefCell<BTreeMap<&'a str, Option<&'a str>>>,
    #[cfg(test)]
    walked: std::cell::Cell<usize>,
}

impl<'a> ThreadRoots<'a> {
    /// Index only `buffer`, even if `loaded` contains other conversations.
    pub fn new(buffer: &'a BufferKey, loaded: impl IntoIterator<Item = &'a Line>) -> Self {
        Self {
            buffer,
            resolved: RefCell::default(),
            #[cfg(test)]
            walked: std::cell::Cell::new(0),
            parents: loaded
                .into_iter()
                .filter(|line| line.buffer == *buffer)
                .filter_map(|line| line.id.as_deref().map(|id| (id, line)))
                .collect(),
        }
    }

    /// The earliest known ancestor. Missing parents retain their ID; empty
    /// references, cycles and lines from another buffer have no root.
    pub fn root(&self, line: &'a Line) -> Option<&'a str> {
        (line.buffer == *self.buffer)
            .then(|| {
                self.indexed_start(line)
                    .map_or_else(|| self.uncached_root(line), |id| self.cached_root(id))
            })
            .flatten()
    }

    fn indexed_start(&self, line: &'a Line) -> Option<&'a str> {
        match line.id.as_deref() {
            None | Some("") => line.reply_to.as_deref(),
            Some(id) => self
                .parents
                .get(id)
                .filter(|held| std::ptr::eq(**held, line))
                .map(|_| id),
        }
    }

    fn cached_root(&self, start: &'a str) -> Option<&'a str> {
        let mut resolved = self.resolved.borrow_mut();
        let mut path = BTreeSet::new();
        let mut node = start;
        let root = loop {
            if node.is_empty() || !path.insert(node) {
                break None;
            }
            if let Some(root) = resolved.get(node) {
                break *root;
            }
            #[cfg(test)]
            self.walked.set(self.walked.get() + 1);
            match self
                .parents
                .get(node)
                .and_then(|line| line.reply_to.as_deref())
            {
                Some(parent) => node = parent,
                None => break Some(node),
            }
        };
        resolved.extend(path.into_iter().map(|id| (id, root)));
        root
    }

    /// Lines outside the snapshot use PR 1's exact traversal, without
    /// caching an unindexed or duplicate-ID relationship into the snapshot.
    fn uncached_root(&self, line: &'a Line) -> Option<&'a str> {
        if line.buffer != *self.buffer {
            return None;
        }
        let mut visited = BTreeSet::new();
        visited.extend(line.id.as_deref());
        let mut root = line.reply_to.as_deref().or(line.id.as_deref())?;
        if line.reply_to.is_none() {
            return (!root.is_empty()).then_some(root);
        }
        loop {
            if root.is_empty() || !visited.insert(root) {
                return None;
            }
            match self
                .parents
                .get(root)
                .and_then(|line| line.reply_to.as_deref())
            {
                Some(parent) => root = parent,
                None => return Some(root),
            }
        }
    }
}

impl Line {
    /// A line without server metadata, ownership or highlights.
    pub fn new(buffer: BufferKey, time: DateTime<Utc>, kind: LineKind, text: String) -> Self {
        Self {
            buffer,
            id: None,
            reply_to: None,
            client_tags: BTreeMap::new(),
            time,
            kind,
            from: None,
            account: None,
            text,
            highlight: false,
            own: false,
        }
    }

    /// The thread root in the loaded lines of this buffer, or the earliest
    /// known ancestor when history is incomplete. A plain message is its
    /// own root. Cyclic or invalid relationships return `None`.
    ///
    /// Resolve on demand: history can arrive after a reply, including after
    /// a restart. No count or server lookup is inferred from these links.
    pub fn thread_root<'a>(&'a self, loaded: &[&'a Line]) -> Option<&'a str> {
        ThreadRoots::new(&self.buffer, loaded.iter().copied()).root(self)
    }

    /// Prepare a literal reply when `message-tags` is enabled and this
    /// message has a server ID. Owners pass their currently loaded lines;
    /// replies to replies target the earliest known root in that buffer.
    pub fn reply(&self, text: &str, loaded: &[&Line], message_tags: bool) -> Option<Command> {
        let message = matches!(
            self.kind,
            LineKind::Privmsg | LineKind::Notice | LineKind::Action
        );
        if !message_tags || !message || self.buffer.target == Target::Server {
            return None;
        }
        self.id.as_deref().filter(|id| !id.is_empty())?;
        Some(Command::Reply {
            buffer: self.buffer.clone(),
            msgid: self.thread_root(loaded)?.to_string(),
            text: text.to_string(),
        })
    }
}
