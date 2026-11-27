//! What answers a pull request without a read: one the account opened,
//! one it lists (with what a read of it found), else what a read said; and
//! which of them want a read all the same.

use subspace_framework::client_github::model::{GhCheck, GhItem};
use subspace_framework::contracts::github::{PullRequestDetails, ResolutionState};

use super::{References, cache_key, details};
use crate::Github;

/// What answers `key` without a read, if anything does: a refusal newer
/// than what the account holds, else a pull request the account opened,
/// else one it listed, with what a read of it already found ([`listed`]),
/// else what a read of it said.
pub(super) fn known(board: &Github, key: &str) -> Option<ResolutionState> {
    let (Some(_), Some(login)) = (&board.client, board.account.as_deref()) else {
        return Some(if board._accounts_task.is_some() {
            ResolutionState::Pending
        } else {
            ResolutionState::SignedOut
        });
    };
    if board.references.signed_out.contains(&login.to_lowercase()) {
        return Some(ResolutionState::SignedOut);
    }
    let cached = cache_key(login, key);
    let read = board.references.answers.get(&cached);
    let newer = newer_than_list(&board.references, &cached);
    if super::access::refused(&board.references, &cached) {
        return Some(ResolutionState::Unavailable);
    }
    match loaded(board, key) {
        Some(Loaded::Opened(item, runs)) => Some(ResolutionState::Available(details(item, runs))),
        Some(Loaded::Listed(item)) => Some(ResolutionState::Available(listed(item, read, newer))),
        // A refusal a later success overtook answers nothing.
        None => read
            .filter(|read| !super::access::is_refusal(read))
            .cloned(),
    }
}

/// Whether the answer for `cached` comes from a read that started after
/// the latest section listing the pull request started, by the one read
/// clock: a section requested later is newer, one requested earlier is
/// not, however the two land.
fn newer_than_list(references: &References, cached: &(String, String)) -> bool {
    let listed = references
        .listed_at
        .get(cached)
        .copied()
        .unwrap_or_default();
    references
        .read_at
        .get(cached)
        .is_some_and(|read| *read > listed)
}

/// A listed pull request's answer. A read newer than the section that
/// lists it (`fresh`) answers whole. An older one lends
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
pub(super) fn wants_read(board: &Github, key: &str) -> bool {
    let Some(login) = board.account.as_deref() else {
        return false;
    };
    let cached = cache_key(login, key);
    let references = &board.references;
    let stale = references.stale.contains(&cached);
    match loaded(board, key) {
        Some(Loaded::Listed(_)) => !references.answers.contains_key(&cached) || stale,
        Some(Loaded::Opened(..)) => stale && super::access::refused(references, &cached),
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
        .find(|detail| {
            same(&detail.item)
                && is_pr(&detail.item)
                && !account.unconfirmed_details.contains(&detail.item.key)
        })
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
