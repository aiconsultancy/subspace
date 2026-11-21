//! The board's view of every network and buffer, built only by applying the
//! runtime's [`Event`]s in order. Pure data: no GPUI, no sockets.

use std::collections::{BTreeMap, HashSet, VecDeque};

use chrono::{DateTime, Duration, Utc};
use subspace_client_irc::{
    Anchor, BufferKey, Command, ConnectionStatus, Event, Line, LineKind, Member, NetworkConfig,
    NetworkId, SendResult, Target, Typing,
};

mod apply;
mod counts;
mod directory;
mod joins;
mod merge;
mod removed;
mod statuses;
#[cfg(test)]
mod tests;

pub(crate) use directory::{CACHE_FILE as DIRECTORY_CACHE, Directories};
pub(crate) use removed::{KEY as REMOVED_KEY, Removed};
pub(crate) use statuses::Statuses;

/// Lines kept per buffer; older live lines are dropped first.
pub(crate) const LINE_CAP: usize = 2000;
/// The most lines a buffer holds after loading older history.
pub(crate) const HISTORY_MAX: usize = 10_000;
/// IRCv3 `+typing`: an active notice expires after 6 s, a paused one after 30 s.
const TYPING_ACTIVE_SECS: i64 = 6;
const TYPING_PAUSED_SECS: i64 = 30;

/// One configured network and what the runtime reported about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Network {
    pub id: NetworkId,
    pub name: String,
    pub status: ConnectionStatus,
    /// Our current nick, or the configured one before registration.
    pub nick: String,
    pub caps: Vec<String>,
    /// Nick-wide status lines while connected; dropped when the connection
    /// ends and never stored.
    pub statuses: Statuses,
}

/// A line with a local sequence number: a stable identity for lines that
/// have no `msgid`. Live lines count up; prepended history counts down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub seq: i64,
    pub line: Line,
    /// The line mentions us, from the runtime or a configured highlight word.
    pub highlight: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Topic {
    pub text: String,
    pub set_by: Option<String>,
}

/// Older-history requests for one buffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct History {
    pub state: HistoryState,
    /// Lines kept beyond `LINE_CAP` because history loaded them.
    pub extra: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum HistoryState {
    /// Older lines may load on request.
    #[default]
    Idle,
    /// A request is running; the runtime answers every one, also when the
    /// connection ends first.
    Loading,
    /// No older lines will load: the server has none, or the buffer holds
    /// `HISTORY_MAX` lines.
    Complete,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Buffer {
    pub entries: VecDeque<Entry>,
    pub topic: Option<Topic>,
    pub members: Vec<Member>,
    /// Nick to the time their typing notice expires.
    pub typing: BTreeMap<String, DateTime<Utc>>,
    pub unread: usize,
    pub highlights: usize,
    /// The retained messages behind the unread count, by local sequence.
    pub unread_entries: HashSet<i64>,
    /// The unread messages shown while reading this buffer.
    opened_unread: HashSet<i64>,
    pub read_marker: Option<DateTime<Utc>>,
    pub joined: bool,
    pub history: History,
}

impl Buffer {
    pub fn shown_unread(&self) -> &HashSet<i64> {
        if self.opened_unread.is_empty() {
            &self.unread_entries
        } else {
            &self.opened_unread
        }
    }
}

/// A live, unread line that may deserve a notification when its buffer is
/// not visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attention {
    pub buffer: BufferKey,
    pub kind: AttentionKind,
    /// The sender's nick.
    pub from: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttentionKind {
    Highlight,
    Query,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct IrcState {
    networks: Vec<Network>,
    buffers: BTreeMap<BufferKey, Buffer>,
    highlights: Vec<String>,
    next_seq: i64,
    first_seq: i64,
    /// A private conversation the user just opened with `/query`, for the
    /// board to show.
    pub(crate) opened: Option<BufferKey>,
    /// Channels left and private conversations closed from the list, which
    /// stay out of it until joined, opened or messaged again. The hub loads
    /// and saves it with the state file.
    pub removed: Removed,
    /// Every network's public channels from its last `LIST`. The hub loads
    /// and saves them in a cache file.
    pub directories: Directories,
    pub joins: joins::Joins,
    /// The runtime's answers to `SendAll` sends, by send ID. The
    /// sender takes its answer.
    sends: BTreeMap<u64, SendResult>,
    /// The last `SendAll` ID handed out.
    last_send: u64,
}

impl IrcState {
    /// The configured networks, each with its server buffer, before any event.
    pub fn new(configs: &[NetworkConfig], highlights: Vec<String>) -> Self {
        let mut state = Self {
            highlights: highlights
                .into_iter()
                .map(|word| word.to_lowercase())
                .filter(|word| !word.is_empty())
                .collect(),
            ..Self::default()
        };
        for config in configs {
            state.networks.push(network(config));
            state
                .buffers
                .insert(server_key(&config.id), Buffer::default());
        }
        state
    }

    /// Follows a new configuration: networks keep their buffers and state
    /// when their id stays, take their new name, and are added or dropped
    /// with their buffers. The runtime reports each restarted network.
    pub fn reconfigure(&mut self, configs: &[NetworkConfig], highlights: Vec<String>) {
        let next = Self::new(configs, highlights);
        self.highlights = next.highlights;
        self.directories.retain(configs);
        self.buffers
            .retain(|key, _| configs.iter().any(|config| config.id == key.network));
        for (key, buffer) in next.buffers {
            self.buffers.entry(key).or_insert(buffer);
        }
        self.networks = next
            .networks
            .into_iter()
            .map(|mut network| {
                if let Some(old) = self.network(&network.id) {
                    network.status = old.status.clone();
                    network.caps = old.caps.clone();
                    if matches!(old.status, ConnectionStatus::Connected { .. }) {
                        network.nick = old.nick.clone();
                        network.statuses = old.statuses.clone();
                    }
                }
                network
            })
            .collect();
    }

    pub fn networks(&self) -> &[Network] {
        &self.networks
    }

    pub fn network(&self, id: &NetworkId) -> Option<&Network> {
        self.networks.iter().find(|network| &network.id == id)
    }

    pub fn buffers(&self) -> impl Iterator<Item = (&BufferKey, &Buffer)> {
        self.buffers
            .iter()
            .filter(|(buffer, _)| !self.joins.retired(buffer))
    }

    pub fn buffer(&self, key: &BufferKey) -> Option<&Buffer> {
        (!self.joins.retired(key))
            .then(|| self.buffers.get(key))
            .flatten()
    }

    /// Our nick on `network`, for completion and own-line checks.
    pub fn nick(&self, network: &NetworkId) -> Option<&str> {
        self.network(network).map(|network| network.nick.as_str())
    }

    /// Applies one runtime event. `now` dates typing expiry and local
    /// status lines. Returns what may need a notification. Events for an
    /// item the user removed are dropped unless they bring it back.
    pub fn apply(&mut self, event: Event, now: DateTime<Utc>) -> Option<Attention> {
        (self.joins.admit(&event) && self.removed.admit(&event))
            .then(|| apply::event(self, event, now))
            .flatten()
    }

    /// A new ID for [`Command::SendAll`](subspace_client_irc::Command::SendAll),
    /// with no answer yet; [`Self::take_send`] returns its answer.
    pub fn next_send(&mut self) -> u64 {
        self.last_send += 1;
        self.sends.remove(&self.last_send);
        self.last_send
    }

    /// The answer to the `SendAll` `id`, once it came; taking it
    /// forgets it.
    pub fn take_send(&mut self, id: u64) -> Option<SendResult> {
        self.sends.remove(&id)
    }

    /// Everything up to the newest line was read here. Returns that line's
    /// time when the marker moved, for `MARKREAD`.
    pub fn mark_read(&mut self, key: &BufferKey) -> Option<DateTime<Utc>> {
        let buffer = self.buffers.get_mut(key)?;
        let latest = buffer.entries.back().map(|entry| entry.line.time)?;
        let moved = buffer.read_marker.is_none_or(|marker| marker < latest);
        let cleared = buffer.unread > 0 || buffer.highlights > 0;
        buffer.read_marker = buffer.read_marker.max(Some(latest));
        buffer.unread = 0;
        buffer.highlights = 0;
        buffer.unread_entries.clear();
        (moved || cleared).then_some(latest)
    }

    /// Keeps the unread messages available to the transcript as it opens.
    pub fn switch_unread(&mut self, previous: Option<&BufferKey>, next: Option<&BufferKey>) {
        if previous == next {
            return;
        }
        if let Some(buffer) = previous.and_then(|key| self.buffers.get_mut(key)) {
            buffer.opened_unread.clear();
        }
        if let Some(buffer) = next.and_then(|key| self.buffers.get_mut(key)) {
            buffer.opened_unread = std::mem::take(&mut buffer.unread_entries);
        }
    }

    /// Opens an older-history request; false while one is running or the
    /// server has no more.
    pub fn begin_history(&mut self, key: &BufferKey) -> bool {
        let Some(buffer) = self.buffers.get_mut(key) else {
            return false;
        };
        if buffer.history.state != HistoryState::Idle {
            return false;
        }
        buffer.history.state = HistoryState::Loading;
        true
    }

    /// The request for the page of history before the oldest line held:
    /// `None` while one runs or when the server has no more.
    pub fn older_history(&mut self, key: &BufferKey, limit: u16) -> Option<Command> {
        if !self.begin_history(key) {
            return None;
        }
        let before = self
            .buffers
            .get(key)
            .and_then(|held| held.entries.front())
            .map(|entry| entry.line.time);
        Some(Command::LoadHistory {
            buffer: key.clone(),
            before,
            limit,
        })
    }

    /// Makes sure buffer `key` of a configured network is in the list, as
    /// opening a stored message of it does. False for a network that is not
    /// configured, the server buffer, or a removed buffer.
    pub fn hold(&mut self, key: &BufferKey) -> bool {
        let wanted = self.network(&key.network).is_some()
            && key.target != Target::Server
            && self.removed.get(key).is_none()
            && !self.joins.retired(key);
        if wanted {
            self.buffers.entry(key.clone()).or_default();
        }
        wanted
    }

    /// Restores `lines`, read back from the store, into buffer `key` of a
    /// configured network, unless the user removed it. They count as
    /// nothing: no unread, no mention, no notification.
    pub fn restore(&mut self, key: &BufferKey, lines: Vec<Line>) {
        if self.network(&key.network).is_none()
            || key.target == Target::Server
            || self.removed.get(key).is_some()
        {
            return;
        }
        merge::merge(self, key, lines, false);
    }

    /// The newest message each channel and private conversation of
    /// `network` holds, for the runtime to resume after.
    pub fn anchors(&self, network: &NetworkId) -> Vec<Anchor> {
        self.buffers
            .iter()
            .filter(|(key, _)| &key.network == network && key.target != Target::Server)
            .filter_map(|(key, buffer)| {
                let newest = buffer
                    .entries
                    .iter()
                    .rev()
                    .find(|entry| is_message(entry.line.kind))?;
                Some(Anchor {
                    target: key.target.clone(),
                    id: newest.line.id.clone(),
                    time: newest.line.time,
                })
            })
            .collect()
    }

    /// Drops typing notices that expired before `now`. True when any did.
    pub fn expire_typing(&mut self, now: DateTime<Utc>) -> bool {
        let mut changed = false;
        for buffer in self.buffers.values_mut() {
            let before = buffer.typing.len();
            buffer.typing.retain(|_, expires| *expires > now);
            changed |= buffer.typing.len() != before;
        }
        changed
    }

    /// Joins channel `key` from the list, which shows it once joined.
    pub fn join_listed(&mut self, key: &BufferKey) -> Command {
        self.joins.request(key);
        let name = match &key.target {
            Target::Channel(name) => name.clone(),
            _ => String::new(),
        };
        Command::Join {
            network: key.network.clone(),
            channel: name,
            key: None,
        }
    }

    /// Takes the query the user opened with `/query`, once.
    pub fn take_opened(&mut self) -> Option<BufferKey> {
        self.opened.take()
    }

    /// Opens (or keeps) a query buffer, as `Command::Query` does, bringing
    /// back one the user closed.
    pub fn open_query(&mut self, network: &NetworkId, nick: &str) -> BufferKey {
        let key = BufferKey {
            network: network.clone(),
            target: Target::Query(nick.to_owned()),
        };
        self.removed.forget(&key);
        self.buffers.entry(key.clone()).or_default();
        key
    }

    /// Removes a channel or private conversation from the list until it is
    /// joined, opened or, for a private conversation, messaged again. The
    /// server buffer stays. False when nothing was removed.
    pub fn remove(&mut self, key: &BufferKey) -> bool {
        if key.target == Target::Server {
            return false;
        }
        let newest = self
            .buffers
            .remove(key)
            .and_then(|buffer| newest_received(&buffer));
        self.removed.insert(key, newest);
        true
    }

    /// Switcher badge: every highlight plus unread private messages.
    pub fn attention_count(&self) -> usize {
        self.buffers()
            .map(|(key, buffer)| match key.target {
                Target::Query(_) => buffer.highlights.max(buffer.unread),
                _ => buffer.highlights,
            })
            .sum()
    }

    /// Networks, and how many are connected, for the status bar.
    pub fn connected_count(&self) -> usize {
        self.networks
            .iter()
            .filter(|network| matches!(network.status, ConnectionStatus::Connected { .. }))
            .count()
    }
}

/// The server buffer's key on `network`.
pub(crate) fn server_key(network: &NetworkId) -> BufferKey {
    BufferKey {
        network: network.clone(),
        target: Target::Server,
    }
}

fn network(config: &NetworkConfig) -> Network {
    Network {
        id: config.id.clone(),
        name: if config.name.is_empty() {
            config.id.0.clone()
        } else {
            config.name.clone()
        },
        status: ConnectionStatus::Disconnected,
        nick: config.nickname.clone(),
        caps: Vec::new(),
        statuses: Statuses::default(),
    }
}

/// The expiry of a typing notice in `state` received at `now`, or `None`
/// when it ends typing.
fn typing_expiry(state: Typing, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match state {
        Typing::Active => Some(now + Duration::seconds(TYPING_ACTIVE_SECS)),
        Typing::Paused => Some(now + Duration::seconds(TYPING_PAUSED_SECS)),
        Typing::Done => None,
    }
}

/// The time of the newest message someone else sent in `buffer`: a time the
/// server set, unlike our own lines, local errors or a late replay appended
/// after newer lines. `None` when there is none.
fn newest_received(buffer: &Buffer) -> Option<DateTime<Utc>> {
    buffer
        .entries
        .iter()
        .filter(|entry| !entry.line.own && is_message(entry.line.kind))
        .map(|entry| entry.line.time)
        .max()
}

/// Lines that count as unread conversation, not membership noise.
pub(crate) fn is_message(kind: LineKind) -> bool {
    matches!(
        kind,
        LineKind::Privmsg | LineKind::Action | LineKind::Notice
    )
}

/// A message from someone else that the runtime flagged or that names a
/// highlight word. Topic, mode and membership lines never highlight.
fn is_highlight(line: &Line, words: &[String]) -> bool {
    !line.own && is_message(line.kind) && (line.highlight || mentions(&line.text, words))
}

/// True when `text` contains one of the lower-case `words` as a whole word.
pub(crate) fn mentions(text: &str, words: &[String]) -> bool {
    let text = subspace_client_irc::format::strip(text).to_lowercase();
    words.iter().any(|word| {
        text.match_indices(word.as_str()).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + word.len()..].chars().next();
            !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
        })
    })
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | '[' | ']' | '\\' | '`' | '^')
}
