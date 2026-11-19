//! Lookups and checks the service's calls share.

use std::collections::BTreeMap;

use crate::{
    Policy, Scope,
    database::{self, Store},
    limits,
};

pub(crate) fn validate_policy(policy: &Policy) -> Result<(), String> {
    if policy.retention_days == 0
        || policy.retention_days > limits::MAX_RETENTION
        || policy.max_bytes < Policy::MIN_MAX_BYTES
        || policy.max_bytes > limits::MAX_QUOTA
    {
        return Err("Search policy is outside supported bounds".into());
    }
    Ok(())
}

pub(crate) fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > limits::MAX_NAME {
        return Err("Invalid search application or board identifier".into());
    }
    Ok(())
}

pub(crate) fn store<'a>(
    stores: &'a BTreeMap<String, Store>,
    board: &str,
) -> Result<&'a Store, String> {
    stores
        .get(board)
        .ok_or_else(|| "Board does not participate in search".into())
}

pub(crate) fn store_mut<'a>(
    stores: &'a mut BTreeMap<String, Store>,
    board: &str,
) -> Result<&'a mut Store, String> {
    stores
        .get_mut(board)
        .ok_or_else(|| "Board does not participate in search".into())
}

pub(crate) fn require_eligible(
    connection: &rusqlite::Connection,
    scope: &Scope,
) -> Result<Policy, String> {
    database::eligible(connection, scope)?
        .ok_or_else(|| "Search scope is inactive or caching is disabled".into())
}
