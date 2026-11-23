//! One rule for who may see a pull request, whichever path read it: every
//! read takes a tick of one clock when it starts (a list section, this
//! Space's own detail read, or each request of a reference read, which
//! reads its keys in turn), and a result is weighed by that tick, never by
//! when it lands.
//!
//! **The invariant.** For each account and pull request the service keeps
//! only two numbers ([`Access`]): the latest start tick of a read that
//! landed with a refusal, and the latest of one that landed with a
//! success. The pull request is refused exactly when the first is the
//! larger, that is, when the most recently started read that has landed
//! with an access decision is a refusal; the resolver then answers
//! `Unavailable` and never a held title, status or counts, and it never
//! answers refused once a later-started read has succeeded. Both numbers
//! only rise, so the answer does not depend on the order reads land in,
//! and nothing else (a refresh, a sign-in or sign-out, an account switch
//! or a reload) lowers or clears them: refusals stay for as long as any
//! retained data could answer. A failure (network, rate limit, signed
//! out) decides nothing. The model test (`access/model.rs`) checks this on
//! every interleaving of six reads.
//!
//! An access refusal is a 400, 403 or 404 that is not a rate limit. A
//! refusal is read again on the next refresh, also for a pull request the
//! account opened ([`super::held::wants_read`]), so one that was about
//! something passing does not stick.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui_kit::Context;
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::core::http::Error;

use super::{References, announce, cache_key, landed, mark_stale, reads};
use crate::Github;

type Key = (String, String);

/// The one read clock. It is shared with the background reads, so each
/// request of a reference read takes its own tick as it starts, after any
/// requests before it in the same batch, not when the batch was queued.
#[derive(Clone, Debug, Default)]
pub(crate) struct Clock(Arc<AtomicU64>);

impl Clock {
    /// The next tick.
    pub(crate) fn tick(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }
}

/// Starts a read: its tick of the one clock.
pub(crate) fn begin(board: &mut Github) -> u64 {
    board.references.clock.tick()
}

/// What a landed read decided about access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Success,
    Refusal,
}

/// The latest start ticks of the reads of one pull request that landed
/// with a refusal and with a success. Both only rise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Access {
    refused: u64,
    confirmed: u64,
}

impl Access {
    /// Records a read that started at `started` and landed with `decision`;
    /// whether it is now the latest of the reads that decided, so its
    /// result may be kept.
    pub(crate) fn land(&mut self, started: u64, decision: Decision) -> bool {
        let latest = started > self.refused.max(self.confirmed);
        let slot = match decision {
            Decision::Success => &mut self.confirmed,
            Decision::Refusal => &mut self.refused,
        };
        *slot = (*slot).max(started);
        latest
    }

    /// Whether the most recently started read that decided was a refusal.
    pub(crate) fn refused(&self) -> bool {
        self.refused > self.confirmed
    }
}

/// Whether `key` is refused now ([`Access::refused`]).
pub(super) fn refused(references: &References, key: &Key) -> bool {
    references.access.get(key).is_some_and(Access::refused)
}

/// Records that a read of `key` that started at `started` landed with
/// `decision`, and whether it is the latest read that decided, so its
/// answer may be kept. A success that makes the pull request readable
/// again drops a held refusal answer.
pub(super) fn decide(
    references: &mut References,
    key: &Key,
    started: u64,
    decision: Decision,
) -> bool {
    let access = references.access.entry(key.clone()).or_default();
    let latest = access.land(started, decision);
    if !access.refused() && references.answers.get(key).is_some_and(is_refusal) {
        references.answers.remove(key);
    }
    latest
}

/// Whether `state` refuses access, rather than saying a read failed or the
/// account is signed out.
pub(super) fn is_refusal(state: &ResolutionState) -> bool {
    matches!(state, ResolutionState::Unavailable)
}

/// List section `section` read by `login`, started at tick `started` and
/// `since` sign-outs, just landed. The first of a refresh marks every
/// answer stale, so each pull request a tile asks about is read again once
/// per refresh. Each pull request it lists counts as read at `started`:
/// newer than a read that started before it, and lifting a refusal only
/// when it started after the refused read.
pub(crate) fn section_landed(
    board: &mut Github,
    login: &str,
    (section, started): (&str, u64),
    since: u64,
    cx: &mut Context<Github>,
) {
    mark_stale(&mut board.references);
    let keys = listed_keys(board, login, section);
    list(&mut board.references, keys, started);
    landed(board, login, since, cx);
}

/// The cache keys of the pull requests `login`'s section `section` lists.
fn listed_keys(board: &Github, login: &str, section: &str) -> Vec<Key> {
    let items = board
        .data
        .get(&login.to_lowercase())
        .and_then(|data| data.sections.get(section))
        .map(|data| data.items.as_slice())
        .unwrap_or_default();
    items
        .iter()
        .map(|item| cache_key(login, &item.key))
        .collect()
}

/// A section that started at `started` lists `keys`.
fn list(references: &mut References, keys: Vec<Key>, started: u64) {
    for key in keys {
        let at = references.listed_at.entry(key.clone()).or_default();
        *at = (*at).max(started);
        decide(references, &key, started, Decision::Success);
    }
}

/// This Space's own read of pull request `key` (and of `canonical`, the
/// key GitHub answered with) by `login`, started at tick `started`,
/// succeeded: it lifts a refusal from a read that started before it.
pub(crate) fn detail_landed(
    board: &mut Github,
    login: &str,
    (key, canonical): (&str, &str),
    started: u64,
) {
    for key in [key, canonical] {
        decide(
            &mut board.references,
            &cache_key(login, key),
            started,
            Decision::Success,
        );
    }
}

/// This Space's own read of pull request `key` by `login`, started at tick
/// `started`, failed. An access refusal answers for it, ahead of the
/// detail and lists held from before, unless a read that started later
/// succeeded; any other failure changes nothing.
pub(crate) fn detail_refused(
    board: &mut Github,
    (login, key): (&str, &str),
    error: &Error,
    started: u64,
    cx: &mut Context<Github>,
) {
    let state = reads::refused(error);
    let references = &mut board.references;
    let key = cache_key(login, key);
    if !is_refusal(&state) || !decide(references, &key, started, Decision::Refusal) {
        return;
    }
    references.read_at.insert(key.clone(), started);
    references.stale.remove(&key);
    references.answers.insert(key, state);
    announce(cx);
}

#[cfg(test)]
mod model;
