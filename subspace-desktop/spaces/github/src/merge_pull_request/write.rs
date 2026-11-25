//! Send one merge request and reconcile its result into the selected account.

use gpui_kit::{AppContext as _, Context, Window};
use subspace_framework::client_github::model::PrState;
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::Toast;

use crate::{Github, emit_toast};

use super::still_current;

pub(super) fn submit(board: &mut Github, window: &mut Window, cx: &mut Context<Github>) {
    let Some(pending) = board.merge_confirmation.as_ref() else {
        return;
    };
    if board.writing || !still_current(board, pending) {
        return;
    }
    let Some(client) = board.client.clone() else {
        return;
    };
    let (login, key, target) = (
        pending.login.clone(),
        pending.item.key.clone(),
        pending.target.clone(),
    );
    board.writing = true;
    cx.notify();
    cx.spawn_in(window, async move |board, cx| {
        let result = cx
            .background_spawn(async move { client.merge_pull_request(&login, &key, &target) })
            .await;
        _ = board.update_in(cx, |board, window, cx| finish(board, result, window, cx));
    })
    .detach();
}

fn finish(
    board: &mut Github,
    result: Result<(), Error>,
    window: &mut Window,
    cx: &mut Context<Github>,
) {
    board.writing = false;
    let Some(pending) = board.merge_confirmation.as_mut() else {
        return;
    };
    let login = pending.login.clone();
    let mut item = pending.item.clone();
    if let Err(error) = result {
        pending.error = Some(match error {
            Error::Status { status: 409, .. } => {
                "The head commit, target branch or merge eligibility changed. Review the current pull request before merging."
            }
            Error::Status { status: 403, .. } => {
                "GitHub denied the merge. Check your permissions and the branch rules."
            }
            Error::Network(_) => "GitHub could not be reached to confirm the merge.",
            _ => "GitHub did not confirm the merge. Check the pull request on GitHub.",
        });
        if let Some(detail) = board
            .data
            .get_mut(&login.to_lowercase())
            .and_then(|account| account.details.get_mut(&item.key))
        {
            detail.merge_target = None;
        }
    } else {
        item.state = PrState::Merged;
        if let Some(account) = board.data.get_mut(&login.to_lowercase()) {
            account.apply_written_state(&item, None);
        }
        board.merge_confirmation = None;
        board.focus.focus(window, cx);
        emit_toast(Toast::info(format!("Merged #{}", item.number)), cx);
        board.save_cache(cx);
        crate::references::announce(cx);
        board.rebuild(cx);
    }
    if board.account.as_deref() == Some(login.as_str()) {
        board.fetch_detail(item.key, false, cx);
    }
    cx.notify();
}
