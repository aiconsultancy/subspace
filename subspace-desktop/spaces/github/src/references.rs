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
//! the head commit; a force-push updates the pull request). A read is
//! taken as newer than the list only when it started after the refresh
//! that marked it stale. The list query is not widened: every list would
//! pay for counts no tile shows.
//!
//! A pull request the account holds neither opened nor listed answers from
//! its read until the next refresh, then is read again once, so one that
//! left the lists, as when the account lost access to its repository, is
//! not answered from memory for good: GitHub's refusal replaces it. A
//! signed-out account starts no read at all.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, Subscription};
use subspace_framework::client_github::Client;
use subspace_framework::client_github::model::{
    CheckRunState, CheckState, GhCheck, GhItem, PrState, ReviewDecision,
};
use subspace_framework::contracts::github::recognition::PullRequestLink;
use subspace_framework::contracts::github::{
    Changed, CheckCounts, CheckStatus, PullRequestDetails, PullRequestStatus, Resolution,
    ResolutionState, ResolvePullRequests, ReviewStatus,
};
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::contracts;
use subspace_framework::dashboard::github_sign_in::{self, SignInEvent};

use crate::Github;
use crate::navigation::requested_pr;

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
}

/// When a read started: the [`References`] generation and refresh count.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Epoch {
    generation: u64,
    refreshes: u64,
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

/// A list section read by `login` that started at `since` just landed:
/// the first of a refresh marks every answer stale, so each listed pull
/// request a tile asks about is read again once per refresh.
pub(crate) fn section_landed(
    board: &mut Github,
    login: &str,
    since: u64,
    cx: &mut Context<Github>,
) {
    mark_stale(&mut board.references);
    landed(board, login, since, cx);
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

/// What answers `key` without a read, if anything does: a pull request
/// the account opened, else one it listed, with what a read of it already
/// found ([`listed`]), else what a read of it said.
fn known(board: &Github, key: &str) -> Option<ResolutionState> {
    let (Some(_), Some(login)) = (&board.client, board.account.as_deref()) else {
        return Some(ResolutionState::SignedOut);
    };
    if board.references.signed_out.contains(&login.to_lowercase()) {
        return Some(ResolutionState::SignedOut);
    }
    let cached = cache_key(login, key);
    let read = board.references.answers.get(&cached);
    let fresh = !board.references.stale.contains(&cached);
    match loaded(board, key) {
        Some(Loaded::Opened(item, runs)) => Some(ResolutionState::Available(details(item, runs))),
        Some(Loaded::Listed(item)) => Some(ResolutionState::Available(listed(item, read, fresh))),
        None => read.cloned(),
    }
}

/// A listed pull request's answer. A read since the last list refresh
/// (`fresh`) is newer than the list and answers whole. An older one lends
/// only its check counts to the list's item, and only while the list says
/// the pull request is as it was read: the same combined checks state and
/// the same last update. A new head, such as a force-push, updates the
/// pull request, so its old head's counts are never shown; the list does
/// not carry the head commit itself.
fn listed(item: &GhItem, read: Option<&ResolutionState>, fresh: bool) -> PullRequestDetails {
    let from_list = details(item, &[]);
    match read {
        Some(ResolutionState::Available(read)) if fresh => read.clone(),
        Some(ResolutionState::Available(read))
            if read.checks == from_list.checks && read.updated_at == from_list.updated_at =>
        {
            from_list.with_check_counts(read.check_counts)
        }
        _ => from_list,
    }
}

/// Whether `key` wants a read although something answers it: a pull
/// request the account holds only from a list, with no read of it since
/// the last refresh (one read gives its runs), or one it does not hold
/// whose read is from before the last refresh, so a pull request that
/// left the lists, as when the account lost access to its repository, is
/// checked again once per refresh rather than answered from memory for
/// good.
fn wants_read(board: &Github, key: &str) -> bool {
    let Some(login) = board.account.as_deref() else {
        return false;
    };
    let cached = cache_key(login, key);
    let references = &board.references;
    let stale = references.stale.contains(&cached);
    match loaded(board, key) {
        Some(Loaded::Listed(_)) => !references.answers.contains_key(&cached) || stale,
        Some(Loaded::Opened(..)) => false,
        None => stale,
    }
}

/// A pull request the current account already holds.
enum Loaded<'a> {
    /// Opened: its detail, with its head commit's check runs.
    Opened(&'a GhItem, &'a [GhCheck]),
    /// Only listed, by a list a refresh has confirmed: its combined checks
    /// state, but not its runs.
    Listed(&'a GhItem),
}

/// The pull request `key` as the current account already loaded it, from a
/// detail or a list a refresh has confirmed.
fn loaded<'a>(board: &'a Github, key: &str) -> Option<Loaded<'a>> {
    let account = board.account_data()?;
    let same = |item: &GhItem| item.key.eq_ignore_ascii_case(key);
    let is_pr = |item: &GhItem| item.kind == subspace_framework::client_github::model::ItemKind::Pr;
    let detail = account
        .details
        .values()
        .find(|detail| same(&detail.item) && is_pr(&detail.item))
        .map(|detail| Loaded::Opened(&detail.item, detail.check_runs.as_slice()));
    let listed = || {
        account
            .sections
            .iter()
            .filter(|(id, _)| !account.unconfirmed.contains(*id))
            .flat_map(|(_, section)| section.items.iter())
            .find(|item| same(item) && is_pr(item))
            .map(Loaded::Listed)
    };
    detail.or_else(listed)
}

/// Starts one background read of the `keys` not already being read, and
/// says what answers them meanwhile.
fn read(board: &mut Github, keys: Vec<String>, cx: &mut Context<Github>) -> ResolutionState {
    let (Some(client), Some(login)) = (board.client.clone(), board.account.clone()) else {
        return ResolutionState::SignedOut;
    };
    let references = &mut board.references;
    // A signed-out account reads nothing, whatever answers the keys.
    if references.signed_out.contains(&login.to_lowercase()) {
        return ResolutionState::SignedOut;
    }
    let fresh: Vec<String> = keys
        .into_iter()
        .filter(|key| references.reading.insert(cache_key(&login, key)))
        .collect();
    let epoch = Epoch {
        generation: references.generation,
        refreshes: references.refreshes,
    };
    if !fresh.is_empty() {
        spawn_read(client, login, fresh, epoch, cx);
    }
    ResolutionState::Pending
}

fn spawn_read(
    client: Arc<Client>,
    login: String,
    keys: Vec<String>,
    epoch: Epoch,
    cx: &mut Context<Github>,
) {
    cx.spawn(async move |this, cx| {
        let account = login.clone();
        let answers = cx
            .background_spawn(async move { read_all(&client, &account, keys) })
            .await;
        _ = this.update(cx, |board, cx| land(board, &login, answers, epoch, cx));
    })
    .detach();
}

/// Reads each key in turn, so one request never fans out into many calls.
fn read_all(client: &Client, login: &str, keys: Vec<String>) -> Vec<(String, ResolutionState)> {
    keys.into_iter()
        .map(|key| {
            let read = client
                .item(login, &key)
                .and_then(|detail| requested_pr((detail, ()), &key));
            let state = match read {
                Ok((detail, ())) => {
                    ResolutionState::Available(details(&detail.item, &detail.check_runs))
                }
                Err(error) => refused(&error),
            };
            (key, state)
        })
        .collect()
}

fn land(
    board: &mut Github,
    login: &str,
    answers: Vec<(String, ResolutionState)>,
    epoch: Epoch,
    cx: &mut Context<Github>,
) {
    let references = &mut board.references;
    if references.generation != epoch.generation {
        return;
    }
    // A read that started before the last refresh is older than the list.
    let older = references.refreshes != epoch.refreshes;
    for (key, state) in answers {
        let key = cache_key(login, &key);
        references.reading.remove(&key);
        if older {
            references.stale.insert(key.clone());
        } else {
            references.stale.remove(&key);
        }
        references.answers.insert(key, state);
    }
    announce(cx);
}

/// What a failed read tells the asker, without saying whether a pull
/// request it cannot read exists.
fn refused(error: &Error) -> ResolutionState {
    match error {
        Error::Credential(_) | Error::Status { status: 401, .. } => ResolutionState::SignedOut,
        Error::Status {
            status: 400 | 403 | 404,
            ..
        } => ResolutionState::Unavailable,
        _ => ResolutionState::Failed,
    }
}

fn cache_key(login: &str, key: &str) -> (String, String) {
    (login.to_lowercase(), key.to_ascii_lowercase())
}

/// The short summary of a loaded pull request, with its check counts when
/// its check runs were read with it.
fn details(item: &GhItem, runs: &[GhCheck]) -> PullRequestDetails {
    let url =
        PullRequestLink::from_key(&item.key).map_or_else(|| item.url.clone(), |link| link.url());
    let updated = Some(item.updated_at.as_str()).filter(|at| !at.is_empty());
    PullRequestDetails::new(&item.key, &item.title, &url, status(item.state))
        .with_author(item.author.as_ref().map(|author| author.login.as_str()))
        .with_review(item.review_decision.map(review))
        .with_checks(item.checks.map(checks))
        .with_check_counts(check_counts(runs))
        .with_updated_at(updated)
        .with_branches(item.head_ref.as_deref(), item.base_ref.as_deref())
        .with_changes(item.additions, item.deletions)
}

/// How many of `runs` passed, out of how many: a check passes unless it
/// failed, was cancelled or is still running, as the detail's check summary
/// counts them. `None` when no runs were read.
fn check_counts(runs: &[GhCheck]) -> Option<CheckCounts> {
    let passed = runs
        .iter()
        .filter(|run| {
            matches!(
                run.state,
                CheckRunState::Success | CheckRunState::Neutral | CheckRunState::Skipped
            )
        })
        .count();
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    (!runs.is_empty()).then(|| CheckCounts::new(count(passed), count(runs.len())))
}

// qual:allow(dry, boilerplate) reason: "Maps the GitHub client's model onto the independent public contract enum; the contract crate must not depend on the client, so neither can derive the other."
fn status(state: PrState) -> PullRequestStatus {
    match state {
        PrState::Open => PullRequestStatus::Open,
        PrState::Draft => PullRequestStatus::Draft,
        PrState::Merged => PullRequestStatus::Merged,
        PrState::Closed => PullRequestStatus::Closed,
    }
}

// qual:allow(dry, repeated_matches) reason: "Maps review decisions to contract values. Copilot Agents maps its own unrelated enums to UI icons and tones."
fn review(decision: ReviewDecision) -> ReviewStatus {
    match decision {
        ReviewDecision::Approved => ReviewStatus::Approved,
        ReviewDecision::ChangesRequested => ReviewStatus::ChangesRequested,
        ReviewDecision::ReviewRequired => ReviewStatus::ReviewRequired,
    }
}

// qual:allow(dry, repeated_matches) reason: "Maps check results to contract values. This enum is independent of review decisions and Copilot Agents UI state."
fn checks(state: CheckState) -> CheckStatus {
    match state {
        CheckState::Success => CheckStatus::Success,
        CheckState::Failure => CheckStatus::Failure,
        CheckState::Pending => CheckStatus::Pending,
    }
}
