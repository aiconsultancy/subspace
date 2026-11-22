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

/// A read by `login` that started at `since` just landed. Its credentials
/// work, so a sign-out since before it started no longer holds; one that
/// came while it ran still does. Either way the other Spaces ask again.
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
        .filter(|(_, state)| state.is_none())
        .map(|(key, _)| key.clone())
        .collect();
    let pending = read(board, unknown, cx);
    states
        .into_iter()
        .map(|(key, state)| Resolution::new(&key, state.unwrap_or_else(|| pending.clone())))
        .collect()
}

/// What answers `key` without a read, if anything does.
fn known(board: &Github, key: &str) -> Option<ResolutionState> {
    let (Some(_), Some(login)) = (&board.client, board.account.as_deref()) else {
        return Some(ResolutionState::SignedOut);
    };
    if board.references.signed_out.contains(&login.to_lowercase()) {
        return Some(ResolutionState::SignedOut);
    }
    if let Some((item, runs)) = loaded(board, key) {
        return Some(ResolutionState::Available(details(item, runs)));
    }
    board
        .references
        .answers
        .get(&cache_key(login, key))
        .cloned()
}

/// The pull request `key` as the current account already loaded it, from a
/// detail or a list a refresh has confirmed, with its head commit's check
/// runs: an opened one has them, a listed one only its combined state, so
/// its runs are empty.
fn loaded<'a>(board: &'a Github, key: &str) -> Option<(&'a GhItem, &'a [GhCheck])> {
    let account = board.account_data()?;
    let same = |item: &GhItem| item.key.eq_ignore_ascii_case(key);
    let detail = account
        .details
        .values()
        .find(|detail| same(&detail.item))
        .map(|detail| (&detail.item, detail.check_runs.as_slice()));
    let listed = || {
        account
            .sections
            .iter()
            .filter(|(id, _)| !account.unconfirmed.contains(*id))
            .flat_map(|(_, section)| section.items.iter())
            .find(|item| same(item))
            .map(|item| (item, &[][..]))
    };
    detail
        .or_else(listed)
        .filter(|(item, _)| item.kind == subspace_framework::client_github::model::ItemKind::Pr)
}

/// Starts one background read of the `keys` not already being read, and
/// says what answers them meanwhile.
fn read(board: &mut Github, keys: Vec<String>, cx: &mut Context<Github>) -> ResolutionState {
    let (Some(client), Some(login)) = (board.client.clone(), board.account.clone()) else {
        return ResolutionState::SignedOut;
    };
    let references = &mut board.references;
    let fresh: Vec<String> = keys
        .into_iter()
        .filter(|key| references.reading.insert(cache_key(&login, key)))
        .collect();
    if !fresh.is_empty() {
        spawn_read(client, login, fresh, references.generation, cx);
    }
    ResolutionState::Pending
}

fn spawn_read(
    client: Arc<Client>,
    login: String,
    keys: Vec<String>,
    generation: u64,
    cx: &mut Context<Github>,
) {
    cx.spawn(async move |this, cx| {
        let account = login.clone();
        let answers = cx
            .background_spawn(async move { read_all(&client, &account, keys) })
            .await;
        _ = this.update(cx, |board, cx| land(board, &login, answers, generation, cx));
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
    generation: u64,
    cx: &mut Context<Github>,
) {
    let references = &mut board.references;
    if references.generation != generation {
        return;
    }
    for (key, state) in answers {
        let key = cache_key(login, &key);
        references.reading.remove(&key);
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
