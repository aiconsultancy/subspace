//! Credential-bound fetched detail caching; section caches do not refresh index age.
use crate::{Github, model, rows};
use gpui_kit::{AppContext as _, Context, Window};
use std::sync::Arc;
use subspace_framework::client_github::{
    BoundRead, Client,
    model::{GhItem, GhItemDetail, ItemKind},
};
use subspace_framework::core::http::Error;
use subspace_framework::dashboard::{
    DashboardEvent, Group, Toast, full_text, full_text_service, search_adapter,
};

pub(super) mod mapping;
mod permits;
pub(super) use permits::purge;
mod load;
pub(super) use load::{Request, load};
mod prefetch_cache;
mod relations;
mod retained;
mod selection;
pub(super) use retained::{append_retained, open};
pub(super) use selection::account_changed;
#[cfg(test)]
mod tests;

const MILLIS_PER_SECOND: i64 = 1_000;

#[derive(Clone)]
struct PrefetchItem {
    item: GhItem,
    refreshed_at: i64,
}

pub(super) fn start_prefetch(board: &mut Github, cx: &mut Context<Github>) {
    board.detail_prefetch = None;
    board.search_generation = board.search_generation.wrapping_add(1);
    if board.account_data().is_some_and(crate::Account::loading) {
        return;
    }
    let (Some(client), Some(login), Some(service)) = (
        board.client.clone(),
        board.account.clone(),
        full_text_service(cx),
    ) else {
        return;
    };
    if !service.policy("github").is_ok_and(|policy| policy.enabled) {
        return;
    }
    let loaded = prefetch_items(board);
    if loaded.is_empty() {
        return;
    }
    let generation = board.search_generation;
    let service_for_index = service.clone();
    cx.spawn(async move |this, cx| {
        let prepared = cx
            .background_spawn(async move {
                bind_prefetch(client.clone(), service, &login).map(|v| (v, client, login))
            })
            .await;
        let _ = this.update(cx, |board, cx| {
            if board.search_generation != generation {
                return;
            }
            let Ok(((source, scope, bound), client, login)) = prepared else {
                return;
            };
            if !use_prefetch_scope(board, generation, &scope, &login, cx) {
                return;
            }
            board.detail_prefetch = Some(subspace_framework::dashboard::prefetch::start(
                loaded,
                source,
                subspace_framework::dashboard::prefetch::Config::default(),
                move |loaded, work| {
                    let ctx = PrefetchBound {
                        bound: &bound,
                        client: &client,
                        login: &login,
                    };
                    let index = search_adapter::Indexing {
                        service: service_for_index.clone(),
                        scope: scope.clone(),
                    };
                    prefetch_one(&ctx, &index, loaded, work)
                },
                cx,
            ));
        });
    })
    .detach();
}

fn use_prefetch_scope(
    board: &mut Github,
    generation: u64,
    scope: &full_text::Scope,
    login: &str,
    cx: &mut Context<Github>,
) -> bool {
    if board
        .search_scope
        .as_ref()
        .is_some_and(|known| known != scope)
    {
        // Do not index the rows captured before the identity changed.
        selection::select(board, generation, &Some(scope.clone()), login, cx);
        return false;
    }
    board.search_scope = Some(scope.clone());
    true
}

fn prefetch_items(board: &Github) -> Vec<PrefetchItem> {
    let Some(account) = board.account_data() else {
        return Vec::new();
    };
    let mut loaded = std::collections::HashMap::new();
    for section in account.sections.values() {
        for item in &section.items {
            loaded
                .entry(item.key.clone())
                .or_insert_with(|| PrefetchItem {
                    item: item.clone(),
                    refreshed_at: section.at.div_euclid(MILLIS_PER_SECOND),
                });
        }
    }
    loaded.into_values().collect()
}

fn bind_prefetch(
    client: Arc<Client>,
    service: full_text::Service,
    login: &str,
) -> Result<
    (
        subspace_framework::dashboard::prefetch::Source,
        full_text::Scope,
        Arc<BoundRead>,
    ),
    String,
> {
    if !service.policy("github")?.enabled {
        return Err("GitHub search caching is disabled".into());
    }
    let bound = Arc::new(client.bind_read(login).map_err(|error| error.to_string())?);
    let scope = mapping::scope(service.app_id(), bound.endpoint(), bound.account_id())
        .ok_or("GitHub returned an invalid verified identity")?;
    let source = subspace_framework::dashboard::prefetch::Source::bind(service, "github", || {
        Ok(scope.clone())
    })?
    .ok_or("GitHub search caching is disabled")?;
    Ok((source, scope, bound))
}

struct PrefetchBound<'a> {
    bound: &'a BoundRead,
    client: &'a Client,
    login: &'a str,
}

fn prefetch_one(
    ctx: &PrefetchBound<'_>,
    index: &search_adapter::Indexing,
    loaded: PrefetchItem,
    work: &subspace_framework::dashboard::prefetch::Work,
) -> Result<(), subspace_framework::dashboard::prefetch::Failure> {
    if prefetch_cache::cached(index, &loaded.item, ctx.bound.account()) {
        return Ok(());
    }
    let list = mapping::list_document(
        &loaded.item,
        loaded.refreshed_at,
        ctx.bound.account_id(),
        ctx.bound.account(),
    )
    .map_err(|_| subspace_framework::dashboard::prefetch::Failure::Transient)?;
    work.publish(&[list])
        .map_err(|_| subspace_framework::dashboard::prefetch::Failure::Transient)?;
    let detail = ctx
        .bound
        .item(&loaded.item.key)
        .map_err(|error| subspace_framework::dashboard::prefetch::Failure::from(&error))?;
    let diff = if loaded.item.kind == ItemKind::Pr {
        Some(
            ctx.client
                .diff(ctx.login, &loaded.item.key)
                .map_err(|error| subspace_framework::dashboard::prefetch::Failure::from(&error))?,
        )
    } else {
        None
    };
    let document = mapping::document_with_diff(
        &detail,
        diff.as_ref(),
        chrono::Utc::now().timestamp(),
        ctx.bound.account_id(),
        ctx.bound.account(),
    )
    .map_err(|_| subspace_framework::dashboard::prefetch::Failure::Transient)?;
    work.publish(&[document])
        .map_err(|_| subspace_framework::dashboard::prefetch::Failure::Transient)?;
    relations::publish(&index.service, &index.scope, &detail.item, &detail.item.key);
    Ok(())
}

pub(super) fn cache_error(cx: &mut Context<Github>) {
    cx.emit(DashboardEvent::Toast(Toast::error(
        "GitHub local search cache could not be updated",
    )));
}

fn bind_failed(board: &mut Github, error: Error, cx: &mut Context<Github>) {
    if matches!(error, Error::Status { status: 403, .. })
        && let Some((service, scope)) = full_text_service(cx).zip(board.search_scope.clone())
    {
        cx.background_spawn(async move {
            let _ = service.invalidate(&scope);
        })
        .detach();
    }
    cache_error(cx);
}

/// Publishes one refreshed detail document and its relation metadata for
/// `scope`, identically to the prefetch and single-item load paths. Never
/// activates the scope: the account must already be the board's active
/// search account, or the write is refused (REQ-19; no refresh grants
/// authority). Used by `agent_tools`'s refresh action so every write path
/// indexes the same way.
pub(crate) fn publish_refreshed(
    service: &full_text::Service,
    scope: &full_text::Scope,
    detail: &GhItemDetail,
    login: &str,
) -> Result<(), String> {
    let document = mapping::document(
        detail,
        chrono::Utc::now().timestamp(),
        &scope.account,
        login,
    )?;
    service.upsert(scope, std::slice::from_ref(&document))?;
    relations::publish(service, scope, &detail.item, &document.id);
    Ok(())
}
