//! Answers `github@1/resolve-pull-requests`: short summaries of the
//! pull requests other Spaces show, read with this Space's current account
//! and its existing credentials. Nothing crosses the contract but the
//! summary. What the account already loaded and confirmed answers at once;
//! anything else is `Pending` while one background read per request fetches
//! it, and `github@1/changed` announces the landing, as it announces every
//! list or detail this Space loads. An account switch, a sign-in, a sign-out
//! or a reload forgets every answer and announces `changed`, so no summary
//! outlives the account that could read it. A signed-out account answers
//! `SignedOut` until it signs in again or a read with it succeeds; lists
//! from before a reload or from the disk cache answer only once a refresh
//! confirms them. Answers stay in memory.
//!
//! A pull request the account holds only from a list has its combined
//! checks state but not its check runs, so a tile would have no bar. The
//! first ask for one answers from the list at once and starts the same
//! one read of it a pull request it does not hold gets, which carries the
//! runs; its landing is announced, and later asks answer with the counts.
//! A refresh of the lists (every section, or the favourites' sections)
//! marks those reads stale when its first section lands, once however
//! many sections land and however many pull requests are opened, so a
//! pull request a tile still asks about is read again at most once per
//! refresh, deduplicated with every other read and bounded by the keys
//! asked about. Until the new read lands a stale read answers only with
//! its counts, on the newer list's item, and only while its combined
//! checks state and last update are the list's (the list does not carry
//! the head commit; a force-push updates the pull request). Every read,
//! whichever path makes it, takes a tick of one clock when it starts
//! ([`access`]); a read is newer than the list only when it started after
//! the latest section listing the pull request started, however the two
//! land. The list query is not widened: every list would pay for counts no
//! tile shows.
//!
//! A pull request the account holds neither opened nor listed answers from
//! its read until the next refresh, then is read again once, so one that
//! left the lists, as when the account lost access to its repository, is
//! not answered from memory for good: GitHub's refusal replaces it. An
//! access refusal (400, 403 or 404; a rate limit is a failure, which the
//! GitHub client reports as 429), from a reference read or this Space's
//! own detail read, answers ahead of a pull request the account opened or
//! listed, until a read that started after it succeeds; a list section or
//! read that started before it never brings the summary back. It is read
//! again on the next refresh. A 401 or a missing credential answers signed
//! out and never overrides held data. A signed-out account starts no read
//! at all.

use std::collections::{HashMap, HashSet};

use gpui_kit::{Context, Subscription};
use subspace_framework::contracts::github::{
    Changed, Resolution, ResolutionState, ResolvePullRequests,
};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::contracts;
use subspace_framework::dashboard::github_sign_in::{self, SignInEvent};

use crate::Github;
pub(crate) use access::{begin, detail_landed, detail_refused, section_landed};
use held::{known, wants_read};
use reads::read;
use summary::details;

mod access;
mod held;
mod reads;
mod summary;

/// What this account learned about pull requests it did not hold, by
/// lower-cased `(account, key)`.
#[derive(Default)]
pub(crate) struct References {
    answers: HashMap<(String, String), ResolutionState>,
    reading: HashSet<(String, String)>,
    /// Bumped by every forget, so a read that started before it is dropped.
    generation: u64,
    /// The lower-cased logins that signed out. Each answers nothing until it
    /// signs in again or a read with it succeeds; choosing it does not count.
    signed_out: HashSet<String>,
    /// Bumped by every sign-out, so a read that started before one never
    /// lifts it.
    sign_outs: u64,
    /// Answers read before the last list refresh: a listed pull request
    /// with one is read again for fresh check runs when next asked.
    stale: HashSet<(String, String)>,
    /// A list refresh started and none of its sections has landed yet:
    /// the first to land marks every answer stale.
    refresh_due: bool,
    /// Bumped each time answers are marked stale, so a read that started
    /// before lands stale: it is older than the list.
    refreshes: u64,
    /// Ticks as each read starts, whichever path makes it: a list section,
    /// this Space's own detail read or a reference read
    /// ([`access::begin`]). Every access decision compares these ticks.
    clock: u64,
    /// For each pull request a section lists, the tick of the latest
    /// section that listed it.
    listed_at: HashMap<(String, String), u64>,
    /// For each answer, the tick its read started at: it is newer than the
    /// list only when it started after the latest section listing the pull
    /// request started.
    read_at: HashMap<(String, String), u64>,
    /// Each pull request the account was refused, with the tick of the
    /// refused read: the refusal answers until a read that started later
    /// succeeds ([`access`]).
    refused_at: HashMap<(String, String), u64>,
    /// Each pull request a read succeeded for, with the latest such read's
    /// tick: a refusal from a read that started before it is superseded.
    confirmed_at: HashMap<(String, String), u64>,
}

/// Answers the contract for `board` and follows sign-in and sign-out. The
/// broker refuses while the Space is not installed; it then never answers.
pub(crate) fn attach(cx: &mut Context<Github>) -> Option<Subscription> {
    let board = cx.entity();
    _ = contracts::respond(Github::ID, &board, answer, cx);
    let sign_in = github_sign_in::shared(cx)?;
    Some(cx.subscribe(&sign_in, |board, _, event: &SignInEvent, cx| {
        signed(board, event, cx);
    }))
}

/// Only the login an event names changes its own guard.
fn signed(board: &mut Github, event: &SignInEvent, cx: &mut Context<Github>) {
    let signed_out = &mut board.references.signed_out;
    match event {
        SignInEvent::SignedOut(login) => signed_out.insert(login.to_lowercase()),
        SignInEvent::SignedIn(login) => signed_out.remove(&login.to_lowercase()),
    };
    if matches!(event, SignInEvent::SignedOut(_)) {
        board.references.sign_outs = board.references.sign_outs.wrapping_add(1);
    }
    forget(board, cx);
}

/// Another account forgets every answer. Choosing an account, even the one
/// already current, never lifts its sign-out.
pub(crate) fn account_chosen(board: &mut Github, changed: bool, cx: &mut Context<Github>) {
    if changed {
        forget(board, cx);
    }
}

/// Taken when a list or detail read starts, for [`landed`].
pub(crate) fn since(board: &Github) -> u64 {
    board.references.sign_outs
}

/// A list refresh starts: its first section to land marks the answers
/// read before it stale ([`section_landed`]).
pub(crate) fn refreshing(board: &mut Github) {
    board.references.refresh_due = true;
}

/// Marks every answer stale, once per refresh.
fn mark_stale(references: &mut References) {
    if std::mem::take(&mut references.refresh_due) {
        references.stale = references.answers.keys().cloned().collect();
        references.refreshes = references.refreshes.wrapping_add(1);
    }
}

/// A read by `login` that started at `since` just landed. Its credentials
/// work, so a sign-out since before it started no longer holds; one that
/// came while it ran still does. Either way the other Spaces ask again.
/// It marks nothing stale: a detail read lands here too.
pub(crate) fn landed(board: &mut Github, login: &str, since: u64, cx: &mut Context<Github>) {
    let references = &mut board.references;
    if references.sign_outs == since {
        references.signed_out.remove(&login.to_lowercase());
    }
    announce(cx);
}

/// Asks the other Spaces to ask again, after this Space's own data changed.
pub(crate) fn announce(cx: &mut Context<Github>) {
    _ = contracts::publish(Github::ID, Changed, cx);
}

/// Forgets every answer and asks the other Spaces to ask again.
pub(crate) fn forget(board: &mut Github, cx: &mut Context<Github>) {
    let references = &mut board.references;
    references.answers.clear();
    references.reading.clear();
    references.stale.clear();
    references.read_at.clear();
    references.refused_at.clear();
    references.confirmed_at.clear();
    references.generation = references.generation.wrapping_add(1);
    announce(cx);
}

fn answer(
    board: &mut Github,
    request: &ResolvePullRequests,
    cx: &mut Context<Github>,
) -> Vec<Resolution> {
    let states: Vec<(String, Option<ResolutionState>)> = request
        .keys()
        .iter()
        .map(|key| (key.clone(), known(board, key)))
        .collect();
    let unknown: Vec<String> = states
        .iter()
        .filter(|(key, state)| state.is_none() || wants_read(board, key))
        .map(|(key, _)| key.clone())
        .collect();
    let pending = read(board, unknown, cx);
    states
        .into_iter()
        .map(|(key, state)| Resolution::new(&key, state.unwrap_or_else(|| pending.clone())))
        .collect()
}

fn cache_key(login: &str, key: &str) -> (String, String) {
    (login.to_lowercase(), key.to_ascii_lowercase())
}
