//! Reuse fetched search content without advancing its age or granting writes.
use subspace_framework::client_github::model::GhItem;
use subspace_framework::dashboard::{full_text, search_adapter::Indexing};

use super::mapping;

const PREFETCH_MAX_AGE_SECONDS: i64 = 15 * 60;

pub(super) fn cached(index: &Indexing, item: &GhItem, login: &str) -> bool {
    let target = full_text::Target {
        scope: index.scope.clone(),
        id: item.key.clone(),
    };
    index
        .service
        .read(&target)
        .ok()
        .flatten()
        .is_some_and(|document| {
            let hit = full_text::Hit {
                target,
                document,
                excerpt: String::new(),
                score: 0.0,
            };
            fresh(&hit, item, login, chrono::Utc::now().timestamp())
        })
}

pub(super) fn fresh(hit: &full_text::Hit, item: &GhItem, login: &str, now: i64) -> bool {
    mapping::retained(hit).is_some_and(|retained| {
        retained.diff_indexed
            && retained.login.eq_ignore_ascii_case(login)
            && retained.detail.item == *item
            && hit.document.refreshed_at <= now
            && now.saturating_sub(hit.document.refreshed_at) < PREFETCH_MAX_AGE_SECONDS
    })
}
