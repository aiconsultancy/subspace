//! One credential-bound item's detail fetch: debounced background read,
//! identity fencing and retained-detail application into the board.
use crate::{DETAIL_DEBOUNCE, Github, error_text, model};
use gpui_kit::{AppContext as _, Context};
use std::sync::Arc;
use subspace_framework::client_github::model::GhItemDetail;
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::{full_text, full_text_service};

use super::{cache_error, mapping};

mod reads;

pub(super) use reads::Prepared;
#[cfg(test)]
pub(super) use reads::prepare;
use reads::{ReadResult, fetch, prepare_read};

#[derive(Clone)]
pub(crate) struct Request {
    pub client: Arc<subspace_framework::client_github::Client>,
    pub login: String,
    pub key: String,
    pub debounce: bool,
    /// [`crate::references::since`] when the read was asked for.
    pub since: u64,
    /// The one read clock: the read takes its tick on the background
    /// thread immediately before its request starts, after the debounce.
    pub clock: crate::references::Clock,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ReadGeneration {
    detail: u64,
    account: u64,
}

fn read_generation(board: &Github, login: &str) -> ReadGeneration {
    ReadGeneration {
        detail: board.detail_generation,
        account: board.read_generation(&login.to_lowercase()),
    }
}

pub(crate) fn load(board: &mut Github, request: Request, cx: &mut Context<Github>) {
    board.detail_generation = board.detail_generation.wrapping_add(1);
    let generation = read_generation(board, &request.login);
    let service = full_text_service(cx);
    let previous_scope = board.search_scope.clone();
    let started = std::time::Instant::now();
    board._detail_task = Some(cx.spawn(async move |this, cx| {
        if request.debounce {
            cx.background_executor().timer(DETAIL_DEBOUNCE).await;
        }
        let (reader, cache) = (request.clone(), service.clone());
        let (tick, (ticket, prepared)) = cx
            .background_spawn(async move {
                prepare_read(&reader, cache.as_ref(), previous_scope.as_ref())
            })
            .await;
        let bound = match prepared {
            Prepared::Bound(bound) => bound,
            Prepared::Live(result, cache_unavailable) => {
                let _ = this.update(cx, |board, cx| {
                    if !is_current(board, generation, &request.login) {
                        return;
                    }
                    let result = (result, cache_unavailable, tick);
                    apply_live(board, &request, result, started, cx);
                });
                return;
            }
        };
        let selected = service.as_ref().and_then(|service| {
            mapping::scope(service.app_id(), bound.endpoint(), bound.account_id())
        });
        let Some((selected_current, generation)) = this
            .update(cx, |board, cx| {
                select_detail(board, generation, &selected, bound.account(), cx)
            })
            .ok()
            .flatten()
        else {
            return;
        };
        let pending = service
            .zip(ticket)
            .zip(selected)
            .filter(|_| selected_current);
        let reader = request.clone();
        let read = cx
            .background_spawn(async move { fetch(&bound, &reader, pending) })
            .await;
        let _ = this.update(cx, |board, cx| {
            if !is_current(board, generation, &request.login) {
                return;
            }
            apply(board, &request, read, started, cx);
        });
    }));
}

fn is_current(board: &Github, generation: ReadGeneration, login: &str) -> bool {
    read_generation(board, login) == generation
        && board
            .account
            .as_deref()
            .is_some_and(|current| model::same_login(current, login))
}

fn select_detail(
    board: &mut Github,
    generation: ReadGeneration,
    scope: &Option<full_text::Scope>,
    login: &str,
    cx: &mut Context<Github>,
) -> Option<(bool, ReadGeneration)> {
    if !is_current(board, generation, login) {
        return None;
    }
    let selected = super::selection::select(board, board.search_generation, scope, login, cx);
    // This read supplied the verified scope and can proceed under its generation.
    Some((selected, read_generation(board, login)))
}

fn apply_live(
    board: &mut Github,
    request: &Request,
    (result, cache_unavailable, tick): (Result<Box<GhItemDetail>, Error>, bool, u64),
    started: std::time::Instant,
    cx: &mut Context<Github>,
) {
    if cache_unavailable {
        cache_error(cx);
    }
    apply(
        board,
        request,
        (result.map(|detail| (*detail, Ok(()))), tick),
        started,
        cx,
    );
}

fn apply(
    board: &mut Github,
    request: &Request,
    (result, tick): (ReadResult, u64),
    started: std::time::Instant,
    cx: &mut Context<Github>,
) {
    let result = result.and_then(|read| crate::navigation::requested_pr(read, &request.key));
    let open = board
        .list
        .open_row()
        .is_some_and(|row| row.key == request.key);
    let selected = board.list.selected().is_some_and(|key| key == &request.key);
    let mut canonical = None;
    match result {
        Ok((detail, cached)) => {
            board
                .retained_permits
                .remove(&(request.login.to_lowercase(), request.key.clone()));
            let account = board.data.entry(request.login.to_lowercase()).or_default();
            let key = detail.item.key.clone();
            account.apply_read_detail(detail, started);
            crate::references::detail_landed(board, &request.login, (&request.key, &key), tick);
            board
                .retained_permits
                .remove(&(request.login.to_lowercase(), key.clone()));
            if crate::navigation::canonicalize(board, &request.key, &key) && selected {
                canonical = Some(key);
            }
            if cached.is_err() {
                cache_error(cx);
            }
            crate::references::landed(board, &request.login, request.since, cx);
        }
        Err(error) => {
            crate::references::detail_refused(
                board,
                (&request.login, &request.key),
                &error,
                tick,
                cx,
            );
            failure(board, &request.key, error_text(&error), cx);
        }
    }
    board.rebuild(cx);
    if let Some(key) = canonical {
        if open {
            board.list.reveal(&key);
        } else {
            board.list.select(key.into());
        }
    }
}

fn failure(board: &mut Github, key: &str, message: String, cx: &mut Context<Github>) {
    if let Some(account) = board.account_data_mut() {
        account.detail_errors.insert(key.into(), message);
        if let Some(detail) = account.details.get_mut(key) {
            detail.merge_target = None;
        }
    }
    cx.notify();
}
