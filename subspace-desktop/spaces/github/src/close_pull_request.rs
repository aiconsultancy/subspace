//! Confirmed close workflow. The draft freezes the target and selected account.

use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EntityInputHandler as _, FocusHandle, KeyBinding,
    Window, actions,
};
use subspace_framework::client_github::model::GhItem;

use crate::Github;

actions!(github_close, [CancelClose, ConfirmClose]);
const CONTEXT: &str = "GithubClose";
mod apply;
mod decision;
mod view;
pub(super) use view::render;
mod write;
pub(super) use decision::is_closable;
use decision::{Submission, opening_allowed, validate_submission};
use write::write;

const COMMENT_MIN_ROWS: usize = 4;
const COMMENT_MAX_ROWS: usize = 12;

pub(super) struct Confirmation {
    item: GhItem,
    login: String,
    comment: Entity<TextareaState>,
    posted: bool,
    error: Option<String>,
    previous_focus: Option<FocusHandle>,
    focus: FocusHandle,
}

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelClose, Some(CONTEXT)),
        KeyBinding::new(
            subspace_framework::dashboard::secondary!("enter"),
            ConfirmClose,
            Some(CONTEXT),
        ),
        KeyBinding::new("escape", CancelClose, Some("GithubClose > Input")),
        KeyBinding::new(
            subspace_framework::dashboard::secondary!("enter"),
            ConfirmClose,
            Some("GithubClose > Input"),
        ),
    ]);
}

pub(super) fn open(board: &mut Github, window: &mut Window, cx: &mut Context<Github>) {
    if !opening_allowed(board.writing, board.close_confirmation.is_some()) {
        return;
    }
    let Some((item, login)) = board
        .subject_item(cx)
        .filter(is_closable)
        .zip(board.account.clone())
    else {
        return;
    };
    show_confirmation(board, item, login, window, cx);
}

fn show_confirmation(
    board: &mut Github,
    item: GhItem,
    login: String,
    window: &mut Window,
    cx: &mut Context<Github>,
) {
    let previous_focus = window.focused(cx);
    let focus = cx.focus_handle();
    let comment = cx.new(|cx| {
        let mut input = TextareaState::new(window, cx).placeholder("Optional comment");
        input.set_auto_grow(COMMENT_MIN_ROWS, COMMENT_MAX_ROWS, cx);
        input
    });
    comment.update(cx, |input, cx| input.focus(window, cx));
    board.close_confirmation = Some(Confirmation {
        item,
        login,
        comment,
        posted: false,
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
    dismiss(board, window, cx);
}

fn dismiss(board: &mut Github, window: &mut Window, cx: &mut Context<Github>) {
    if let Some(pending) = board.close_confirmation.take() {
        if let Some(focus) = pending.previous_focus {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}

pub(super) fn submit(board: &mut Github, window: &mut Window, cx: &mut Context<Github>) {
    let Some(pending) = board.close_confirmation.as_ref() else {
        return;
    };
    let composing = pending.comment.update(cx, |input, cx| {
        input.marked_text_range(window, cx).is_some()
    });
    let text = pending.comment.read(cx).value().to_string();
    match validate_submission(board.writing, composing, &text, board.client.is_some()) {
        Submission::Ignore => {}
        Submission::Invalid(error) => show_error(board, error.into(), cx),
        Submission::Ready => start_write(board, text, window, cx),
    }
}

fn show_error(board: &mut Github, error: String, cx: &mut Context<Github>) {
    if let Some(pending) = board.close_confirmation.as_mut() {
        pending.error = Some(error);
    }
    cx.notify();
}

fn start_write(board: &mut Github, text: String, window: &mut Window, cx: &mut Context<Github>) {
    let Some(client) = board.client.clone() else {
        return;
    };
    let Some(pending) = board.close_confirmation.as_mut() else {
        return;
    };
    let (login, key, posted) = (
        pending.login.clone(),
        pending.item.key.clone(),
        pending.posted,
    );
    board.writing = true;
    pending.error = None;
    cx.notify();
    cx.spawn_in(window, async move |board, cx| {
        let outcome = cx
            .background_spawn(async move {
                let outcome = write(&client, &login, &key, &text, posted);
                let fresh = outcome.result.is_ok().then(|| client.item(&login, &key));
                (outcome, fresh)
            })
            .await;
        _ = board.update_in(cx, |board, window, cx| {
            apply::finish(board, outcome, window, cx)
        });
    })
    .detach();
}

#[cfg(test)]
mod tests;
