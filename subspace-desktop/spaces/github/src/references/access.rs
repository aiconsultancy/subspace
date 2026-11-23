//! One rule for who may see a pull request, whichever path read it: every
//! read takes a tick of one clock when it starts (a list section, this
//! Space's own detail read, or a reference read), and a result is weighed
//! by that tick, not by when it lands.
//!
//! - An access refusal (400, 403 or 404 that is not a rate limit) answers
//!   for the pull request, ahead of what the account listed or opened,
//!   until a read that started later than the refused one succeeds.
//! - A success from a read that started before the refusal changes
//!   nothing: a list section requested while the account still had access
//!   never brings the summary back.
//! - A refusal from a read that started before a later success is
//!   superseded and dropped.
//!
//! A refusal is read again on the next refresh, also for a pull request
//! the account opened ([`super::held::wants_read`]), so one that was about
//! something passing does not stick.

use gpui_kit::Context;
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::core::http::Error;

use super::{References, announce, cache_key, landed, mark_stale, reads};
use crate::Github;

type Key = (String, String);

/// Starts a read: its tick of the one clock.
pub(crate) fn begin(board: &mut Github) -> u64 {
    tick(&mut board.references)
}

fn tick(references: &mut References) -> u64 {
    references.clock = references.clock.wrapping_add(1);
    references.clock
}

/// Whether a read of `key` that started at `started` and succeeded is
/// newer than any refusal; if so it lifts the refusal and is recorded.
pub(super) fn confirm(references: &mut References, key: &Key, started: u64) -> bool {
    if references
        .refused_at
        .get(key)
        .is_some_and(|at| *at > started)
    {
        return false;
    }
    if references.refused_at.remove(key).is_some() {
        references.answers.remove(key);
    }
    let at = references.confirmed_at.entry(key.clone()).or_default();
    *at = (*at).max(started);
    true
}

/// Whether a refusal of `key` from a read that started at `started` is
/// newer than any success; if so it is recorded.
pub(super) fn refuse(references: &mut References, key: &Key, started: u64) -> bool {
    if references
        .confirmed_at
        .get(key)
        .is_some_and(|at| *at > started)
    {
        return false;
    }
    references.refused_at.insert(key.clone(), started);
    true
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
        confirm(references, &key, started);
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
        confirm(&mut board.references, &cache_key(login, key), started);
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
    if !is_refusal(&state) || !refuse(references, &key, started) {
        return;
    }
    references.read_at.insert(key.clone(), started);
    references.stale.remove(&key);
    references.answers.insert(key, state);
    announce(cx);
}
