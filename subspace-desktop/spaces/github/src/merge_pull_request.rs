//! A single, confirmed merge of the head GitHub showed to the selected account.

use gpui_kit::{App, Context, FocusHandle, KeyBinding, Window, actions};
use subspace_framework::client_github::model::{GhItem, MergeTarget, PrState};

use crate::Github;

mod view;
pub(super) use view::{button, render};
mod write;
use write::submit;

actions!(github_merge, [CancelMerge]);
const CONTEXT: &str = "GithubMerge";

pub(super) struct Confirmation {
    item: GhItem,
    login: String,
    target: MergeTarget,
    error: Option<&'static str>,
    previous_focus: Option<FocusHandle>,
    focus: FocusHandle,
}

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CancelMerge, Some(CONTEXT))]);
}

fn target<'a>(board: &'a Github, key: &str) -> Option<&'a MergeTarget> {
    let detail = board.account_data()?.details.get(key)?;
    (detail.item.state == PrState::Open)
        .then_some(detail.merge_target.as_ref())
        .flatten()
}

fn open(board: &mut Github, key: &str, window: &mut Window, cx: &mut Context<Github>) {
    if board.writing || board.close_confirmation.is_some() || board.merge_confirmation.is_some() {
        return;
    }
    let Some((item, (login, target))) = board
        .account_data()
        .and_then(|account| account.details.get(key))
        .map(|detail| detail.item.clone())
        .zip(board.account.clone().zip(target(board, key).cloned()))
    else {
        return;
    };
    let previous_focus = window.focused(cx);
    let focus = cx.focus_handle();
    window.focus(&focus, cx);
    board.merge_confirmation = Some(Confirmation {
        item,
        login,
        target,
        error: None,
        previous_focus,
        focus,
    });
    cx.notify();
}

pub(super) fn cancel(board: &mut Github, window: &mut Window, cx: &mut Context<Github>) {
    if board.writing {
        return;
    }
    if let Some(pending) = board.merge_confirmation.take() {
        if let Some(focus) = pending.previous_focus {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}

fn still_current(board: &Github, pending: &Confirmation) -> bool {
    board.account.as_deref() == Some(&pending.login)
        && board.visible_detail_key() == Some(&pending.item.key)
        && target(board, &pending.item.key) == Some(&pending.target)
        && board.find_item(&pending.item.key).is_some_and(|item| {
            item.state == PrState::Open && item.base_ref == pending.item.base_ref
        })
}
