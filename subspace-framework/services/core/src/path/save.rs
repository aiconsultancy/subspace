//! Saving a known-good `PATH` at startup.

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use super::cache::{self, CACHE_VERSION, CacheSource, PathCache, write_cache};
use super::capture::Capture;
use super::entries::distinct;
use super::resolve::Resolved;
use crate::log::{Level, Logger};

/// Save a known-good `PATH` and use `file` for tool lookups; without a
/// file, do nothing. A failed save is logged at debug level and changes
/// nothing else.
pub(super) fn save(
    file: Option<PathBuf>,
    resolved: &Resolved,
    previous: Option<PathCache>,
    now: DateTime<Utc>,
) {
    let Some(file) = file else {
        return;
    };
    let saved = cache_to_save(resolved, previous.as_ref(), now);
    if let Some(saved) = &saved
        && let Err(error) = write_cache(&file, saved)
    {
        Logger::new("path").record(
            Level::Debug,
            "path cache not saved",
            &[("error", &error.kind().to_string())],
        );
    }
    cache::activate(file, saved.as_ref().or(previous.as_ref()));
}

/// The cache to save for `resolved`: after a login-shell capture, a
/// registry read or a start from a terminal, the `PATH` without the
/// fallback folders, keeping the tool paths of `previous`. `None` for any
/// other capture.
pub(super) fn cache_to_save(
    resolved: &Resolved,
    previous: Option<&PathCache>,
    now: DateTime<Utc>,
) -> Option<PathCache> {
    let source = match &resolved.capture {
        Capture::Captured { shell, .. } => CacheSource::LoginShell {
            shell: shell.clone(),
        },
        Capture::Terminal => CacheSource::Terminal,
        Capture::Registry => CacheSource::Registry,
        _ => return None,
    };
    Some(PathCache {
        version: CACHE_VERSION.to_string(),
        saved_at: now,
        source,
        path: distinct(&resolved.path)
            .into_iter()
            .filter(|entry| !resolved.fallback.contains(entry))
            .collect(),
        tools: previous
            .map(|previous| previous.tools.clone())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests;
