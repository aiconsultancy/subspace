//! Apply a completed close attempt to its frozen account and visible state.
use super::write::{Outcome, failure_message};
use super::{dismiss, show_error};
use crate::{Github, emit_toast, model};
use gpui_kit::{Context, Window};
use subspace_framework::client_github::model::{GhItemDetail, PrState};
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::Toast;

pub(super) fn finish(
    board: &mut Github,
    (outcome, fresh): (Outcome, Option<Result<GhItemDetail, Error>>),
    window: &mut Window,
    cx: &mut Context<Github>,
) {
    board.writing = false;
    let Some(pending) = board.close_confirmation.as_mut() else {
        return;
    };
    pending.posted = outcome.posted;
    match outcome.result {
        Err(error) => show_error(board, failure_message(outcome.posted, &error), cx),
        Ok(()) => finish_closed(board, fresh.and_then(Result::ok), window, cx),
    }
}

fn finish_closed(
    board: &mut Github,
    fresh: Option<GhItemDetail>,
    window: &mut Window,
    cx: &mut Context<Github>,
) {
    apply_closed(board, fresh, cx);
    dismiss(board, window, cx);
    // Closing removes its trigger; return to board navigation after success.
    board.focus.focus(window, cx);
}

fn apply_closed(board: &mut Github, fresh: Option<GhItemDetail>, cx: &mut Context<Github>) {
    let Some(pending) = &board.close_confirmation else {
        return;
    };
    let mut item = fresh
        .as_ref()
        .map_or_else(|| pending.item.clone(), |detail| detail.item.clone());
    item.state = PrState::Closed;
    if let Some(account) = board.data.get_mut(&pending.login.to_lowercase()) {
        account
            .closed_at
            .insert(item.key.clone(), std::time::Instant::now());
        model::patch_sections(&mut account.sections, &item);
        if let Some(mut detail) = fresh {
            detail.item = item.clone();
            account.details.insert(item.key.clone(), detail);
        } else if let Some(detail) = account.details.get_mut(&item.key) {
            detail.item = item.clone();
        }
    }
    if let Some((diff, _)) = &board.diff {
        diff.update(cx, |diff, cx| diff.update_item(&item, cx));
    }
    emit_toast(Toast::info(format!("Closed #{}", item.number)), cx);
    board.save_cache(cx);
    crate::references::announce(cx);
    board.rebuild(cx);
}
