//! What a buffer keeps about its requests apart from their lines: the
//! account each request's agent is pinned to, the requests still waiting
//! for one, and the requests that ended. Kept in the Space's state, so
//! folding again from the held lines uses the same pins, and an ending
//! outlives its notice.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::state::HISTORY_MAX;

/// The most requests a buffer remembers a pin, an ending or unfolded
/// progress for, oldest forgotten first. A buffer holds at most
/// [`HISTORY_MAX`] lines, so this covers every held request and as many
/// that are no longer held.
const REMEMBERED: usize = 2 * HISTORY_MAX;

/// A held request: its message ID, the nick it asks in ASCII lower case,
/// and the account the server reports for that nick now, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Asked {
    pub id: String,
    pub nick: String,
    pub account: Option<String>,
}

/// A held progress notice and the request it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    pub seq: i64,
    pub request: String,
    /// The server vouches for it: its request is held and pinned, and the
    /// notice comes from the pinned account.
    pub trusted: bool,
    /// It says the request completed or failed.
    pub ended: bool,
}

/// What a live line brings to its buffer's requests, decided before it is
/// held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Arrival {
    Progress(Notice),
    Request(Asked),
    Other,
}

impl Arrival {
    /// Trusted progress, which never counts as unread.
    pub(crate) fn folds(&self) -> bool {
        matches!(self, Self::Progress(notice) if notice.trusted)
    }
}

/// Values by request message ID that keep the first value set and forget
/// the oldest beyond [`REMEMBERED`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Recent<V> {
    values: HashMap<String, V>,
    order: VecDeque<String>,
}

impl<V> Recent<V> {
    fn get(&self, id: &str) -> Option<&V> {
        self.values.get(id)
    }

    /// Sets `id` to `value` unless it has one. True when it was new.
    fn insert(&mut self, id: &str, value: V) -> bool {
        if self.values.contains_key(id) {
            return false;
        }
        self.values.insert(id.to_owned(), value);
        self.order.push_back(id.to_owned());
        if self.order.len() > REMEMBERED
            && let Some(oldest) = self.order.pop_front()
        {
            self.values.remove(&oldest);
        }
        true
    }
}

/// One buffer's requests. See `docs/changes/irc-thread-status/spec.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Requests {
    /// The held requests, by message ID, with the nick each asks.
    held: HashMap<String, String>,
    /// The account each request's agent held when Channels first saw the
    /// request with an account reported for it. Only that account's notices
    /// fold, whoever holds the nick later.
    pins: Recent<String>,
    /// Held requests without a pin yet, by the nick they ask.
    waiting: BTreeMap<String, BTreeSet<String>>,
    /// Requests that held progress answers without folding, so that the
    /// buffer folds again once one is pinned or held again.
    strays: Recent<()>,
    /// Requests that completed or failed, for good, also once the notice
    /// that said so is no longer held.
    ended: Recent<()>,
    /// The folded notices of each held request, by its message ID.
    notices: HashMap<String, Vec<i64>>,
}

impl Requests {
    pub(crate) fn held(&self, id: &str) -> bool {
        self.held.contains_key(id)
    }

    /// The account request `id` is pinned to.
    pub(crate) fn pin(&self, id: &str) -> Option<&str> {
        self.pins.get(id).map(String::as_str)
    }

    pub(crate) fn ended(&self, id: &str) -> bool {
        self.ended.get(id).is_some()
    }

    /// The nicks held requests ask that have no pin yet.
    pub(crate) fn waiting(&self) -> impl Iterator<Item = &str> {
        self.waiting.keys().map(String::as_str)
    }

    /// Holds a request, pinned to the reported account when it has no pin
    /// yet, else waiting for one. True when it is newly held with a pin and
    /// held progress answers it, so the buffer must fold again.
    pub(crate) fn hold(&mut self, asked: &Asked) -> bool {
        let fresh = self
            .held
            .insert(asked.id.clone(), asked.nick.clone())
            .is_none();
        let pinned = self.pins.get(&asked.id).is_some()
            || asked
                .account
                .as_ref()
                .is_some_and(|account| self.pins.insert(&asked.id, account.clone()));
        if !pinned {
            self.waiting
                .entry(asked.nick.clone())
                .or_default()
                .insert(asked.id.clone());
        }
        fresh && pinned && self.strays.get(&asked.id).is_some()
    }

    /// Pins every request waiting for `nick` to `account`. True when held
    /// progress answers one of them, so the buffer must fold again.
    pub(crate) fn pin_waiting(&mut self, nick: &str, account: &str) -> bool {
        let ids = self.waiting.remove(nick).unwrap_or_default();
        ids.iter()
            .for_each(|id| _ = self.pins.insert(id, account.to_owned()));
        ids.iter().any(|id| self.strays.get(id).is_some())
    }

    /// Records a held progress notice: folded into `folded` when trusted,
    /// with its request's ending; otherwise remembered as unfolded.
    pub(crate) fn note(&mut self, notice: &Notice, folded: &mut HashSet<i64>) {
        if !notice.trusted {
            self.strays.insert(&notice.request, ());
            return;
        }
        folded.insert(notice.seq);
        self.notices
            .entry(notice.request.clone())
            .or_default()
            .push(notice.seq);
        if notice.ended {
            self.ended.insert(&notice.request, ());
        }
    }

    /// Takes in a live line now held. True when the buffer must fold again.
    pub(crate) fn arrive(&mut self, arrival: &Arrival, folded: &mut HashSet<i64>) -> bool {
        match arrival {
            Arrival::Progress(notice) => {
                self.note(notice, folded);
                false
            }
            Arrival::Request(asked) => self.hold(asked),
            Arrival::Other => false,
        }
    }

    /// Starts again from `asked`, every request the buffer holds, keeping
    /// pins and endings.
    pub(crate) fn rehold(&mut self, asked: &[Asked]) {
        self.held.clear();
        self.waiting.clear();
        self.notices.clear();
        asked.iter().for_each(|asked| _ = self.hold(asked));
    }

    /// Folds `notices`, every progress notice the buffer holds, into
    /// `folded` afresh.
    pub(crate) fn refold(&mut self, notices: &[Notice], folded: &mut HashSet<i64>) {
        folded.clear();
        notices.iter().for_each(|notice| self.note(notice, folded));
    }

    /// Forgets request `id`, no longer held: it waits no more, and its
    /// notices no longer fold. Returns their sequences. Its pin and ending
    /// stay, should it load again.
    pub(crate) fn release(&mut self, id: &str) -> Vec<i64> {
        if let Some(nick) = self.held.remove(id)
            && let Some(ids) = self.waiting.get_mut(&nick)
        {
            ids.remove(id);
            if ids.is_empty() {
                self.waiting.remove(&nick);
            }
        }
        let notices = self.notices.remove(id).unwrap_or_default();
        if !notices.is_empty() {
            self.strays.insert(id, ());
        }
        notices
    }
}
