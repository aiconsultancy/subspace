//! What a buffer keeps about its requests apart from their lines: the
//! account each request is bound to, and the requests that ended. Kept in
//! the Space's state and in the store ([`Kept`]), so folding again from the
//! held lines, or after a restart, uses the same bindings, and an ending
//! outlives its notice.
//!
//! A request's binding is, in this order: the account the member lists
//! report for the nick it asks when it arrives live; otherwise the server
//! `account` tag on the earliest progress notice that nick sends on it. An
//! account equal to the requester's own is never a binding. Without a
//! binding nothing on the request folds.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::state::HISTORY_MAX;

/// The most requests a buffer remembers a binding, an ending or unfolded
/// progress for, oldest forgotten first, in memory and in the store. A
/// buffer holds at most [`HISTORY_MAX`] lines, so this covers every held
/// request and as many that are no longer held.
pub(crate) const REMEMBERED: usize = 2 * HISTORY_MAX;

/// A held request: its message ID, the nick it asks in ASCII lower case,
/// the requester's server account, and, for a request arriving live, the
/// account the member lists report for that nick now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Asked {
    pub id: String,
    pub nick: String,
    pub requester: Option<String>,
    pub account: Option<String>,
}

/// A held progress notice and the request it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    pub seq: i64,
    pub request: String,
    /// It says the request completed or failed.
    pub ended: bool,
    /// The sender's nick in ASCII lower case.
    pub from: Option<String>,
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

/// One request's binding and ending, as the store keeps them. An older
/// store without them loads with none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Kept {
    pub request: String,
    #[serde(default)]
    pub pin: Option<String>,
    #[serde(default)]
    pub ended: bool,
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

/// A held request's nick and requester.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Held {
    nick: String,
    requester: Option<String>,
}

/// One buffer's requests. See `docs/changes/irc-thread-status/spec.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Requests {
    /// The held requests, by message ID.
    held: HashMap<String, Held>,
    /// Each request's binding: only that account's notices fold, whoever
    /// holds the nick later.
    pins: Recent<String>,
    /// Held requests whose earliest notice from the nick they ask was
    /// taken in, bound or not.
    answered: HashSet<String>,
    /// Requests that held progress answers without folding, so that the
    /// buffer folds again once one is held again.
    strays: Recent<()>,
    /// Requests that completed or failed, for good, also once the notice
    /// that said so is no longer held.
    ended: Recent<()>,
    /// The folded notices of each held request, by its message ID.
    notices: HashMap<String, Vec<i64>>,
    /// Requests whose binding or ending changed since the store last took
    /// them.
    changed: BTreeSet<String>,
}

/// Whether `account` is the requester's own.
fn own(requester: Option<&str>, account: &str) -> bool {
    requester.is_some_and(|requester| requester.eq_ignore_ascii_case(account))
}

impl Requests {
    /// The account request `id` is bound to.
    pub(crate) fn pin(&self, id: &str) -> Option<&str> {
        self.pins.get(id).map(String::as_str)
    }

    pub(crate) fn ended(&self, id: &str) -> bool {
        self.ended.get(id).is_some()
    }

    /// Whether `notice` folds: its request is held and bound, by an earlier
    /// binding or by this notice itself, and it comes from that account.
    pub(crate) fn trusts(&self, notice: &Notice) -> bool {
        matches!(
            (notice.account.as_deref(), self.pin_for(notice)),
            (Some(sent), Some(pin)) if sent.eq_ignore_ascii_case(pin)
        )
    }

    /// The binding `notice` is checked against.
    fn pin_for<'a>(&'a self, notice: &'a Notice) -> Option<&'a str> {
        if !self.held.contains_key(&notice.request) {
            return None;
        }
        self.pin(&notice.request)
            .or_else(|| self.binding(notice).filter(|_| self.first_answer(notice)))
    }

    /// Whether `notice` is the first notice taken in on its unbound, held
    /// request from the nick that request asks.
    fn first_answer(&self, notice: &Notice) -> bool {
        let from_asked = self
            .held
            .get(&notice.request)
            .is_some_and(|held| notice.from.as_deref() == Some(held.nick.as_str()));
        from_asked
            && self.pins.get(&notice.request).is_none()
            && !self.answered.contains(&notice.request)
    }

    /// The account `notice` would bind its request to: its own, unless it
    /// is the requester's.
    fn binding<'n>(&self, notice: &'n Notice) -> Option<&'n str> {
        let requester = self.held.get(&notice.request)?.requester.as_deref();
        notice
            .account
            .as_deref()
            .filter(|account| !own(requester, account))
    }

    /// Binds request `id` to `account` unless it has a binding.
    fn bind(&mut self, id: &str, account: &str) {
        if self.pins.insert(id, account.to_owned()) {
            self.changed.insert(id.to_owned());
        }
    }

    /// Binds `notice`'s request to its account when it is the first
    /// answer from the nick the request asks.
    fn answer(&mut self, notice: &Notice) {
        if !self.first_answer(notice) {
            return;
        }
        self.answered.insert(notice.request.clone());
        if let Some(account) = self.binding(notice) {
            self.bind(&notice.request, account);
        }
    }

    /// Holds a request, bound to the account reported as it arrives live
    /// unless that is the requester's own. True when it is newly held and
    /// held progress answers it, so the buffer must fold again.
    pub(crate) fn hold(&mut self, asked: &Asked) -> bool {
        let held = Held {
            nick: asked.nick.clone(),
            requester: asked.requester.clone(),
        };
        let fresh = self.held.insert(asked.id.clone(), held).is_none();
        let reported = asked.account.as_deref();
        if let Some(account) = reported.filter(|account| !own(asked.requester.as_deref(), account))
        {
            self.bind(&asked.id, account);
        }
        fresh && self.strays.get(&asked.id).is_some()
    }

    /// Records a held progress notice: folded into `folded` when trusted,
    /// with its request's ending; otherwise remembered as unfolded.
    pub(crate) fn note(&mut self, notice: &Notice, folded: &mut HashSet<i64>) {
        let trusted = self.trusts(notice);
        self.answer(notice);
        if !trusted {
            self.strays.insert(&notice.request, ());
            return;
        }
        folded.insert(notice.seq);
        self.notices
            .entry(notice.request.clone())
            .or_default()
            .push(notice.seq);
        if notice.ended && self.ended.insert(&notice.request, ()) {
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
    /// bindings and endings.
    pub(crate) fn rehold(&mut self, asked: &[Asked]) {
        self.held.clear();
        self.answered.clear();
        self.notices.clear();
        asked.iter().for_each(|asked| _ = self.hold(asked));
    }

    /// Folds `notices`, every progress notice the buffer holds, oldest
    /// first by server time, into `folded` afresh.
    pub(crate) fn refold(&mut self, notices: &[Notice], folded: &mut HashSet<i64>) {
        folded.clear();
        notices.iter().for_each(|notice| self.note(notice, folded));
    }

    /// Forgets request `id`, no longer held: its notices no longer fold.
    /// Returns their sequences. Its binding and ending stay, should it
    /// load again.
    pub(crate) fn release(&mut self, id: &str) -> Vec<i64> {
        self.held.remove(id);
        self.answered.remove(id);
        let notices = self.notices.remove(id).unwrap_or_default();
        if !notices.is_empty() {
            self.strays.insert(id, ());
        }
        notices
    }

    /// Loads the bindings and endings the store kept, oldest first, before
    /// the buffer's lines fold, so a stored binding wins.
    pub(crate) fn load(&mut self, kept: &[Kept]) {
        for kept in kept {
            if let Some(pin) = &kept.pin {
                self.pins.insert(&kept.request, pin.clone());
            }
            if kept.ended {
                self.ended.insert(&kept.request, ());
            }
        }
    }

    /// The requests whose binding or ending changed since the last call,
    /// for the store.
    pub(crate) fn take_changes(&mut self) -> Vec<Kept> {
        std::mem::take(&mut self.changed)
            .into_iter()
            .map(|request| Kept {
                pin: self.pin(&request).map(str::to_owned),
                ended: self.ended(&request),
                request,
            })
            .collect()
    }
}
