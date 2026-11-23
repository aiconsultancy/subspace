//! The detail read's work on the background thread: binding the
//! credential, the request itself (each taking its tick of the read clock
//! as it starts) and publishing to the local search cache.

use subspace_framework::client_github::model::GhItemDetail;
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::full_text;

use super::Request;
use crate::model;
use crate::search_index::{mapping, relations};

pub(super) type PendingIndex = (
    (full_text::Service, full_text::ActivationTicket),
    full_text::Scope,
);
pub(super) type ReadResult = Result<(GhItemDetail, Result<(), String>), Error>;

pub(super) fn prepare_read(
    request: &Request,
    service: Option<&full_text::Service>,
    previous: Option<&full_text::Scope>,
) -> (u64, (Option<full_text::ActivationTicket>, Prepared)) {
    let ticket = service.and_then(|service| service.begin_activation("github").ok());
    // Its tick as the request starts: a live read is made here.
    let tick = request.clock.tick();
    let prepared = prepare(
        &request.client,
        service,
        previous,
        &request.login,
        &request.key,
    );
    (tick, (ticket, prepared))
}

pub(super) fn fetch(
    bound: &subspace_framework::client_github::BoundRead,
    request: &Request,
    pending: Option<PendingIndex>,
) -> (ReadResult, u64) {
    let lease = pending.as_ref().and_then(|((service, ticket), scope)| {
        let activation = service.prepare_activation_ticket(ticket, scope).ok()?;
        service.activate_prepared(&activation).ok()?;
        service.lease(scope).ok()
    });
    // Its tick as the request starts, after the lease is in place.
    let tick = request.clock.tick();
    let detail = match bound.item(&request.key) {
        Ok(detail) => detail,
        Err(_) => {
            let read = request
                .client
                .item(&request.login, &request.key)
                .map(|detail| (detail, Err("Bound read unavailable".into())));
            return (read, tick);
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
    (Ok((detail, cached)), tick)
}

pub(in crate::search_index) enum Prepared {
    Bound(subspace_framework::client_github::BoundRead),
    Live(Result<Box<GhItemDetail>, Error>, bool),
}

pub(in crate::search_index) fn prepare(
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
