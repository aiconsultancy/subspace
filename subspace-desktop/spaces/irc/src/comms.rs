//! Notification summaries for mentions and thread replies.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, Target, ThreadRoots};

use crate::state::{Buffer, Entry, IrcState, is_message};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub buffer: BufferKey,
    pub root: String,
}

#[derive(Clone)]
pub(crate) struct Summary {
    pub key: Key,
    pub author: String,
    pub subject: String,
    pub preview: String,
    pub time: DateTime<Utc>,
    pub mentions: bool,
    pub replies: bool,
    unread: Vec<(i64, bool)>,
}

impl Summary {
    pub(crate) fn unread(&self) -> usize {
        self.unread.len()
    }

    pub(crate) fn unread_mention(&self) -> bool {
        self.unread.iter().any(|(_, mention)| *mention)
    }
}

/// The most recent summaries for each current buffer. Read state is kept
/// separately from the IRC channel's read marker for the lifetime of the hub.
#[derive(Default)]
pub(crate) struct Inbox {
    channels: HashMap<BufferKey, Channel>,
}

#[derive(Default)]
struct Channel {
    stamp: Option<(usize, i64, i64, usize)>,
    unread: HashSet<i64>,
    read: HashSet<i64>,
    summaries: Vec<Rc<Summary>>,
}

impl Inbox {
    pub(crate) fn refresh(&mut self, state: &IrcState) {
        self.channels
            .retain(|buffer, _| state.buffer(buffer).is_some() && !state.joins.retired(buffer));
        for (key, buffer) in state.buffers() {
            if matches!(key.target, Target::Channel(_)) && !state.joins.retired(key) {
                self.channels
                    .entry(key.clone())
                    .or_default()
                    .refresh(key, buffer);
            }
        }
    }

    pub(crate) fn summaries(&self) -> Vec<Rc<Summary>> {
        let mut rows: Vec<_> = self.rows().cloned().collect();
        rows.sort_by(|a, b| {
            b.time
                .cmp(&a.time)
                .then(a.key.buffer.network.0.cmp(&b.key.buffer.network.0))
                .then(a.key.root.cmp(&b.key.root))
        });
        rows
    }

    pub(crate) fn unread(&self) -> (usize, bool) {
        self.rows().fold((0, false), |(count, at), row| {
            (count + row.unread(), at || row.unread_mention())
        })
    }

    pub(crate) fn read(&mut self, key: Option<&Key>) {
        if let Some(key) = key {
            if let Some(channel) = self.channels.get_mut(&key.buffer) {
                channel.read(Some(key));
            }
        } else {
            self.channels
                .values_mut()
                .for_each(|channel| channel.read(None));
        }
    }

    fn rows(&self) -> impl Iterator<Item = &Rc<Summary>> {
        self.channels
            .values()
            .flat_map(|channel| &channel.summaries)
    }
}

impl Channel {
    fn refresh(&mut self, key: &BufferKey, buffer: &Buffer) {
        let Some((first, last)) = buffer.entries.front().zip(buffer.entries.back()) else {
            *self = Self::default();
            return;
        };
        let stamp = (
            buffer.entries.len(),
            first.seq,
            last.seq,
            buffer.folded.len(),
        );
        if self.stamp == Some(stamp) {
            return;
        }
        self.stamp = Some(stamp);
        let loaded: HashSet<_> = buffer.entries.iter().map(|entry| entry.seq).collect();
        self.read.retain(|seq| loaded.contains(seq));
        self.unread.retain(|seq| loaded.contains(seq));
        self.unread
            .extend(buffer.unread_entries.difference(&self.read).copied());
        self.summaries = summaries(key, buffer, (&self.unread, &self.read))
            .into_values()
            .map(Rc::new)
            .collect();
    }

    fn read(&mut self, key: Option<&Key>) {
        for row in &mut self.summaries {
            if row.unread.is_empty() || key.is_some_and(|key| *key != row.key) {
                continue;
            }
            let row = Rc::make_mut(row);
            for (seq, _) in row.unread.drain(..) {
                self.read.insert(seq);
                self.unread.remove(&seq);
            }
        }
    }
}

fn summaries(
    key: &BufferKey,
    buffer: &Buffer,
    (unread, read): (&HashSet<i64>, &HashSet<i64>),
) -> HashMap<String, Summary> {
    let entries = buffer
        .entries
        .iter()
        .filter(|entry| is_message(entry.line.kind));
    let roots = ThreadRoots::new(key, entries.clone().map(|entry| &entry.line));
    let original: HashMap<_, _> = entries
        .clone()
        .filter_map(|entry| Some((entry.line.id.as_deref()?, entry)))
        .collect();
    let participated: HashSet<_> = entries
        .clone()
        .filter(|entry| entry.line.own)
        .filter_map(|entry| roots.root(&entry.line))
        .collect();
    let recent = entries
        .filter(|entry| !entry.line.own && !buffer.folded.contains(&entry.seq))
        .filter_map(|entry| {
            let root = roots.root(&entry.line)?;
            let reply = entry.line.reply_to.is_some()
                && (participated.contains(root)
                    || unread.contains(&entry.seq)
                    || read.contains(&entry.seq));
            (entry.highlight || reply).then_some((root, entry, reply))
        });
    collect_summaries(key, &original, recent, unread)
}

fn collect_summaries<'a>(
    key: &BufferKey,
    original: &HashMap<&str, &'a Entry>,
    activity: impl Iterator<Item = (&'a str, &'a Entry, bool)>,
    unread: &HashSet<i64>,
) -> HashMap<String, Summary> {
    let mut rows = HashMap::new();
    for (root, entry, reply) in activity {
        let original = original.get(root).copied().unwrap_or(entry);
        let row = rows
            .entry(root.to_owned())
            .or_insert_with(|| summary(key, root, original, entry));
        apply_activity(row, entry, reply, unread.contains(&entry.seq));
    }
    rows
}

fn apply_activity(row: &mut Summary, entry: &Entry, reply: bool, unread: bool) {
    let excerpt = excerpt(entry);
    let line = &entry.line;
    record_activity(
        row,
        entry,
        (excerpt, line.from.clone().unwrap_or_default()),
        reply,
        unread,
    );
}

fn record_activity(
    row: &mut Summary,
    entry: &Entry,
    (preview, author): (String, String),
    reply: bool,
    unread: bool,
) {
    if entry.line.time >= row.time {
        row.preview = preview;
        row.author = author;
        row.time = entry.line.time;
    }
    row.mentions |= entry.highlight;
    row.replies |= reply;
    if unread {
        row.unread.push((entry.seq, entry.highlight));
    }
}

fn summary(buffer: &BufferKey, root: &str, original: &Entry, entry: &Entry) -> Summary {
    Summary {
        key: Key {
            buffer: buffer.clone(),
            root: root.to_owned(),
        },
        author: String::new(),
        subject: excerpt(original),
        preview: String::new(),
        time: entry.line.time,
        mentions: false,
        replies: false,
        unread: Vec::new(),
    }
}

fn excerpt(entry: &Entry) -> String {
    let text = subspace_client_irc::format::strip(&entry.line.text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
