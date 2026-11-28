//! Reading the captain's snapshot in the background when the join wants
//! it, again on a timer while no live stream carries the events, and
//! again after a failure once its backoff is over.

use std::time::Duration;

use gpui_kit::{AppContext as _, Context, Task};
use subspace_client_fleet::activity::AgentActivity;
use subspace_client_fleet::{Error, FleetClient};

use super::{POLL, retry, sync};
use crate::Irc;

/// Reads the snapshot in the background when the join wants it and none
/// is read yet; whether one started.
pub(super) fn start_fetch(
    board: &mut Irc,
    client: Option<FleetClient>,
    cx: &mut Context<Irc>,
) -> bool {
    let Some((selection, client)) = board.inspector_activity.as_mut().zip(client) else {
        return false;
    };
    if !selection.join.start_fetch() {
        return false;
    }
    let agent = selection.agent.clone();
    selection.fetch = Some(cx.spawn(async move |board, cx| {
        let result = cx
            .background_spawn(async move { client.activity(&agent, None) })
            .await;
        _ = board.update(cx, |board, cx| finish_fetch(board, result, cx));
    }));
    true
}

/// Takes the fetch's answer, lets go of items whose turn left the
/// snapshot and, after a failure, sets the retry's timer.
pub(super) fn finish_fetch(
    board: &mut Irc,
    result: Result<AgentActivity, Error>,
    cx: &mut Context<Irc>,
) {
    if let Some(selection) = board.inspector_activity.as_mut() {
        selection.join.finish_fetch(result);
        selection.fetch = None;
        super::load::prune(selection);
        selection.retry = selection.join.retry_in().map(|wait| retry_after(wait, cx));
    }
    sync(board, cx);
    cx.notify();
}

/// Retries the failed fetch after `wait`, unless it was retried first or
/// the view closed: the task goes with the selection.
fn retry_after(wait: Duration, cx: &mut Context<Irc>) -> Task<()> {
    cx.spawn(async move |board, cx| {
        cx.background_executor().timer(wait).await;
        _ = board.update(cx, retry);
    })
}

/// Reads the snapshot again on a timer while no live stream carries the
/// captain's events.
pub(super) fn poll(cx: &mut Context<Irc>) -> Task<()> {
    cx.spawn(async move |board, cx| {
        loop {
            cx.background_executor().timer(POLL).await;
            if board.update(cx, refresh).is_err() {
                return;
            }
        }
    })
}

fn refresh(board: &mut Irc, cx: &mut Context<Irc>) {
    let Some(selection) = board.inspector_activity.as_mut() else {
        return;
    };
    if !selection.join.live() {
        selection.join.refresh();
        sync(board, cx);
    }
}
