//! One credential-bound item's detail fetch: debounced background read,
//! identity fencing and retained-detail application into the board.
use crate::{DETAIL_DEBOUNCE, Github, error_text, model};
use gpui_kit::{AppContext as _, Context};
use std::sync::Arc;
use subspace_framework::client_github::model::GhItemDetail;
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::{full_text, full_text_service};

use super::{cache_error, mapping, relations};

#[derive(Clone)]
pub(crate) struct Request {
    pub client: Arc<subspace_framework::client_github::Client>,
    pub login: String,
    pub key: String,
    pub debounce: bool,
    /// [`crate::references::since`] when the read was asked for.
    pub since: u64,
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
        let (ticket, prepared) = cx
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
                    apply_live(board, &request, (result, cache_unavailable), started, cx);
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
        let result = cx
            .background_spawn(async move { fetch(&bound, &reader, pending) })
            .await;
        let _ = this.update(cx, |board, cx| {
            if !is_current(board, generation, &request.login) {
                return;
            }
            apply(board, &request, result, started, cx);
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

type PendingIndex = (
    (full_text::Service, full_text::ActivationTicket),
    full_text::Scope,
);
type ReadResult = Result<(GhItemDetail, Result<(), String>), Error>;

fn apply_live(
    board: &mut Github,
    request: &Request,
    (result, cache_unavailable): (Result<Box<GhItemDetail>, Error>, bool),
    started: std::time::Instant,
    cx: &mut Context<Github>,
) {
    if cache_unavailable {
        cache_error(cx);
    }
    apply(
        board,
        request,
        result.map(|detail| (*detail, Ok(()))),
        started,
        cx,
    );
}

fn prepare_read(
    request: &Request,
    service: Option<&full_text::Service>,
    previous: Option<&full_text::Scope>,
) -> (Option<full_text::ActivationTicket>, Prepared) {
    let ticket = service.and_then(|service| service.begin_activation("github").ok());
    (
        ticket,
        prepare(
            &request.client,
            service,
            previous,
            &request.login,
            &request.key,
        ),
    )
}

fn fetch(
    bound: &subspace_framework::client_github::BoundRead,
    request: &Request,
    pending: Option<PendingIndex>,
) -> ReadResult {
    let lease = pending.as_ref().and_then(|((service, ticket), scope)| {
        let activation = service.prepare_activation_ticket(ticket, scope).ok()?;
        service.activate_prepared(&activation).ok()?;
        service.lease(scope).ok()
    });
    let detail = match bound.item(&request.key) {
        Ok(detail) => detail,
        Err(_) => {
            return request
                .client
                .item(&request.login, &request.key)
                .map(|detail| (detail, Err("Bound read unavailable".into())));
        }
    };
    let cached = publish(
        pending
            .as_ref()
            .map(|((service, _), scope)| (service, scope)),
        lease.as_ref(),
        &detail,
        bound,
    );
    Ok((detail, cached))
}

pub(super) enum Prepared {
    Bound(subspace_framework::client_github::BoundRead),
    Live(Result<Box<GhItemDetail>, Error>, bool),
}

pub(super) fn prepare(
    client: &subspace_framework::client_github::Client,
    service: Option<&full_text::Service>,
    previous: Option<&full_text::Scope>,
    login: &str,
    key: &str,
) -> Prepared {
    if service.is_some_and(|service| service.policy("github").is_ok_and(|policy| policy.enabled)) {
        match client.bind_read(login) {
            Ok(bound) if model::same_login(bound.account(), login) => {
                return Prepared::Bound(bound);
            }
            Ok(_) => return Prepared::Live(client.item(login, key).map(Box::new), true),
            Err(error) => {
                if matches!(error, Error::Status { status: 403, .. })
                    && let Some((service, scope)) = service.zip(previous)
                {
                    let _ = service.invalidate(scope);
                }
                return Prepared::Live(client.item(login, key).map(Box::new), true);
            }
        }
    }
    Prepared::Live(client.item(login, key).map(Box::new), false)
}

fn apply(
    board: &mut Github,
    request: &Request,
    result: ReadResult,
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
        Err(error) => failure(board, &request.key, error_text(&error), cx),
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

fn publish(
    indexing: Option<(&full_text::Service, &full_text::Scope)>,
    lease: Option<&full_text::Lease>,
    detail: &GhItemDetail,
    bound: &subspace_framework::client_github::BoundRead,
) -> Result<(), String> {
    let Some(((service, scope), lease)) = indexing.zip(lease) else {
        return Ok(());
    };
    let document = mapping::document(
        detail,
        chrono::Utc::now().timestamp(),
        bound.account_id(),
        bound.account(),
    )?;
    service.upsert_leased(lease, std::slice::from_ref(&document))?;
    relations::publish(service, scope, &detail.item, &document.id);
    Ok(())
}

fn failure(board: &mut Github, key: &str, message: String, cx: &mut Context<Github>) {
    if let Some(account) = board.account_data_mut() {
        account.detail_errors.insert(key.into(), message);
    }
    cx.notify();
}
