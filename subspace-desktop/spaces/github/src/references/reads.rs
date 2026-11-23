//! Reading pull requests the PRs Space does not hold well enough to
//! answer: one background read per request of the keys not already being
//! read, each key in turn, dropped if the account changed meanwhile and
//! landing stale if a list refresh came while it ran.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context};
use subspace_framework::client_github::Client;
use subspace_framework::contracts::github::ResolutionState;
use subspace_framework::core::http::Error;

use super::{announce, cache_key, details};
use crate::Github;
use crate::navigation::requested_pr;

/// When a read started: the [`super::References`] generation, refresh
/// count and its tick of the one read clock ([`super::access`]).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Epoch {
    generation: u64,
    refreshes: u64,
    started: u64,
}

/// Starts one background read of the `keys` not already being read, and
/// says what answers them meanwhile.
pub(super) fn read(
    board: &mut Github,
    keys: Vec<String>,
    cx: &mut Context<Github>,
) -> ResolutionState {
    let (Some(client), Some(login)) = (board.client.clone(), board.account.clone()) else {
        return ResolutionState::SignedOut;
    };
    let started = super::access::begin(board);
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
        started,
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
        if !current(references, &key, &state, epoch.started) {
            continue;
        }
        if older {
            references.stale.insert(key.clone());
        } else {
            references.stale.remove(&key);
        }
        references.read_at.insert(key.clone(), epoch.started);
        references.answers.insert(key, state);
    }
    announce(cx);
}

/// Whether `state`, read for `key` from a read that started at `started`,
/// still stands: a success after any refusal, a refusal after any success
/// ([`super::access`]). Other answers always do.
fn current(
    references: &mut super::References,
    key: &(String, String),
    state: &ResolutionState,
    started: u64,
) -> bool {
    match state {
        ResolutionState::Available(_) => super::access::confirm(references, key, started),
        ResolutionState::Unavailable => super::access::refuse(references, key, started),
        _ => true,
    }
}

/// What a failed read tells the asker, without saying whether a pull
/// request it cannot read exists.
pub(super) fn refused(error: &Error) -> ResolutionState {
    match error {
        Error::Credential(_) | Error::Status { status: 401, .. } => ResolutionState::SignedOut,
        Error::Status {
            status: 400 | 403 | 404,
            ..
        } => ResolutionState::Unavailable,
        _ => ResolutionState::Failed,
    }
}
