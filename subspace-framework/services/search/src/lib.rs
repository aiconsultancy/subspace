//! Private, opt-in, local full-text search over explicitly selected board scopes.
mod admission;
mod api;
mod database;
mod deadline;
mod fenced_read;
mod files;
mod hidden;
mod limits;
mod links;
mod maintenance;
mod metadata;
mod migration;
mod query;
mod ranking;
mod records;
mod revision;
mod selection;
mod service;
mod sources;
mod stores;
mod writes;

pub use api::*;
pub use links::{link, parse_link};
pub use revision::{CachedRead, ReadPermit};
pub use selection::{Activation, ActivationTicket};
pub use service::Service;

fn storage_error(_: impl std::fmt::Display) -> String {
    "Local search storage operation failed".into()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs().min(i64::MAX as u64) as i64)
}
