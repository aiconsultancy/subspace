//! What a buffer keeps about its requests apart from their lines: the
//! requests it holds with the nick each asks, the notices on them, and the
//! requests that ended. Endings are also stored, so a finished request
//! stays finished after a restart or a partial reload.
//!
//! A notice is trusted when its server `account` tag equals the nick its
//! request asks, ignoring ASCII case. See
//! `docs/changes/irc-thread-status/spec.md`.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};

use super::Outcome;
use crate::state::HISTORY_MAX;

/// The most endings a buffer remembers, oldest forgotten first, in memory
/// and in the store. A buffer holds at most [`HISTORY_MAX`] lines, so this
/// covers every held request and as many that are no longer held.
pub(crate) const REMEMBERED: usize = 2 * HISTORY_MAX;

/// A held request: its message ID and the nick it asks in ASCII lower case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Asked {
    pub id: String,
    pub nick: String,
}

/// How and when a request ended, as far as is known: an ending stored
/// before outcomes were kept has neither.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Ending {
    pub outcome: Option<Outcome>,
    /// The server time of the notice that ended it.
    pub at: Option<DateTime<Utc>>,
}

/// A held progress notice and the request it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    pub seq: i64,
    pub request: String,
    /// How the request ended, when the notice says it completed or failed.
    pub ending: Option<Ending>,
    /// The sender's account, from the server's `account` tag.
    pub account: Option<String>,
}

/// What a live line brings to its buffer's requests, decided before it is
/// held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Arrival {
    /// Progress, `trusted` when it folds.
    Progress {
        notice: Notice,
        trusted: bool,
    },
    Request(Asked),
    Other,
}

impl Arrival {
    /// Trusted progress, which never counts as unread.
    pub(crate) fn folds(&self) -> bool {
        matches!(self, Self::Progress { trusted: true, .. })
    }
}

/// Ended requests by message ID, with how each ended, oldest forgotten
/// beyond [`REMEMBERED`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Recent {
    endings: HashMap<String, Ending>,
    order: VecDeque<String>,
}

impl Recent {
    fn get(&self, id: &str) -> Option<&Ending> {
        self.endings.get(id)
    }

    /// Adds `id`'s `ending`, or fills in one known without its outcome.
    /// True when that changed what is kept.
    fn insert(&mut self, id: &str, ending: Ending) -> bool {
        if let Some(held) = self.endings.get_mut(id) {
            let fills = held.outcome.is_none() && ending.outcome.is_some();
            if fills {
                *held = ending;
            }
            return fills;
        }
        self.endings.insert(id.to_owned(), ending);
        self.order.push_back(id.to_owned());
        if self.order.len() > REMEMBERED
            && let Some(oldest) = self.order.pop_front()
        {
            self.endings.remove(&oldest);
        }
        true
    }
}

/// One buffer's requests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Requests {
    /// The held requests, by message ID, with the nick each asks.
    held: HashMap<String, String>,
    /// The folded notices of each held request, by its message ID.
    notices: HashMap<String, Vec<i64>>,
    /// The held progress notices that did not fold, by the request ID they
    /// name, so the buffer folds again when that request is held anew. An
    /// entry goes with the last of its notices.
    unfolded: HashMap<String, HashSet<i64>>,
    /// Requests that completed or failed, for good, also once the notice
    /// that said so is no longer held.
    ended: Recent,
    /// Requests that ended since the store last took them.
    changed: BTreeSet<String>,
}

impl Requests {
    /// How request `id` ended, once it has.
    pub(crate) fn ending(&self, id: &str) -> Option<&Ending> {
        self.ended.get(id)
    }

    /// The nick held request `id` asks, in ASCII lower case.
    pub(crate) fn asked(&self, id: &str) -> Option<&str> {
        self.held.get(id).map(String::as_str)
    }

    /// Whether `notice` folds: its request is held, and its server account
    /// is the nick that request asks.
    pub(crate) fn trusts(&self, notice: &Notice) -> bool {
        let asked = self.held.get(&notice.request);
        matches!(
            (notice.account.as_deref(), asked),
            (Some(account), Some(nick)) if account.eq_ignore_ascii_case(nick)
        )
    }

    /// Holds a request. True when it is newly held and held progress names
    /// it, so the buffer must fold again.
    pub(crate) fn hold(&mut self, asked: &Asked) -> bool {
        let fresh = self
            .held
            .insert(asked.id.clone(), asked.nick.clone())
            .is_none();
        fresh && self.unfolded.contains_key(&asked.id)
    }

    /// Records a held progress notice: folded into `folded` when trusted,
    /// with its request's ending; otherwise kept as unfolded.
    pub(crate) fn note(&mut self, notice: &Notice, folded: &mut HashSet<i64>) {
        if !self.trusts(notice) {
            self.unfolded
                .entry(notice.request.clone())
                .or_default()
                .insert(notice.seq);
            return;
        }
        folded.insert(notice.seq);
        self.notices
            .entry(notice.request.clone())
            .or_default()
            .push(notice.seq);
        if let Some(ending) = notice.ending
            && self.ended.insert(&notice.request, ending)
        {
            self.changed.insert(notice.request.clone());
        }
    }

    /// Takes in a live line now held. True when the buffer must fold again.
    pub(crate) fn arrive(&mut self, arrival: &Arrival, folded: &mut HashSet<i64>) -> bool {
        match arrival {
            Arrival::Progress { notice, .. } => {
                self.note(notice, folded);
                false
            }
            Arrival::Request(asked) => self.hold(asked),
            Arrival::Other => false,
        }
    }

    /// Starts again from `asked`, every request the buffer holds, keeping
    /// endings.
    pub(crate) fn rehold(&mut self, asked: &[Asked]) {
        self.held.clear();
        self.notices.clear();
        self.unfolded.clear();
        asked.iter().for_each(|asked| _ = self.hold(asked));
    }

    /// Folds `notices`, every progress notice the buffer holds, into
    /// `folded` afresh.
    pub(crate) fn refold(&mut self, notices: &[Notice], folded: &mut HashSet<i64>) {
        folded.clear();
        notices.iter().for_each(|notice| self.note(notice, folded));
    }

    /// Forgets request `id`, no longer held: its notices no longer fold.
    /// Returns their sequences. Its ending stays, should it load again.
    pub(crate) fn release(&mut self, id: &str) -> Vec<i64> {
        self.held.remove(id);
        let notices = self.notices.remove(id).unwrap_or_default();
        if !notices.is_empty() {
            self.unfolded
                .entry(id.to_owned())
                .or_default()
                .extend(notices.iter().copied());
        }
        notices
    }

    /// Forgets notice `seq` on request `request`, no longer held.
    pub(crate) fn forget_notice(&mut self, request: &str, seq: i64) {
        if let Some(seqs) = self.notices.get_mut(request) {
            seqs.retain(|held| *held != seq);
        }
        if let Some(seqs) = self.unfolded.get_mut(request) {
            seqs.remove(&seq);
            if seqs.is_empty() {
                self.unfolded.remove(request);
            }
        }
    }

    /// Loads the endings the store kept, oldest first.
    pub(crate) fn load(&mut self, ended: &[(String, Ending)]) {
        ended
            .iter()
            .for_each(|(id, ending)| _ = self.ended.insert(id, *ending));
    }

    /// The requests that ended since the last call, with how each ended,
    /// for the store.
    pub(crate) fn take_endings(&mut self) -> Vec<(String, Ending)> {
        std::mem::take(&mut self.changed)
            .into_iter()
            .filter_map(|id| {
                let ending = *self.ended.get(&id)?;
                Some((id, ending))
            })
            .collect()
    }
}
