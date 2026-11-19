//! The `PATH` that worked last time, and where each tool was found.
//!
//! The file (`path-cache.json` beside the product's `state.json`) holds only
//! a schema version, the save time, the source, the `PATH` folders and tool
//! paths. It never holds network or proxy variables, other environment
//! values or tokens. [`super::apply`] saves it when the `PATH` is known good:
//! after a login-shell capture, or after a start from a terminal.
//!
//! The GUI, the service and the CLI can write at the same time. Each write
//! goes to a temporary file that is renamed over the old one, so the file
//! stays valid; one process can overwrite a tool path another just saved.
//! That lost update is accepted: the next successful lookup saves it again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::log::{Level, Logger};

/// The schema this build reads and writes. Another value is a foreign file.
pub(super) const CACHE_VERSION: &str = "1";

/// Where a saved `PATH` came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CacheSource {
    LoginShell {
        shell: PathBuf,
    },
    Terminal,
    /// Windows: the machine and user `Path` registry values.
    Registry,
}

/// The cache file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathCache {
    pub version: String,
    pub saved_at: DateTime<Utc>,
    pub source: CacheSource,
    /// The `PATH` folders, in order.
    pub path: Vec<PathBuf>,
    /// The last absolute path where each tool was found, as found: no
    /// symbolic link is resolved.
    #[serde(default)]
    pub tools: BTreeMap<String, PathBuf>,
}

/// A saved `PATH` that [`super::apply`] used because the shell failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPath {
    pub saved_at: DateTime<Utc>,
    pub source: CacheSource,
    pub version: String,
    /// How many saved folders were appended.
    pub added: usize,
}

/// A saved tool path that a lookup tried after `PATH` had no such tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedTool {
    pub path: PathBuf,
    /// When the cache file was saved; the tool was found then or later.
    pub saved_at: DateTime<Utc>,
    /// Whether the file still exists and is executable, so it was used.
    pub usable: bool,
}

/// The cache in `file`, or `None` when it is missing, unreadable, not
/// JSON, of another schema or of the wrong shape. Such a file is replaced
/// at the next good save.
/// A relative tool path in the file is dropped: it depends on a working
/// directory that is not known.
pub fn read_cache(file: &Path) -> Option<PathCache> {
    let bytes = std::fs::read(file).ok()?;
    let mut cache = serde_json::from_slice::<PathCache>(&bytes)
        .ok()
        .filter(|cache| cache.version == CACHE_VERSION)?;
    cache.tools.retain(|_, path| path.is_absolute());
    Some(cache)
}

/// Write `cache` to `file` through a private temporary file in the same
/// folder and a rename.
pub(super) fn write_cache(file: &Path, cache: &PathCache) -> std::io::Result<()> {
    use std::io::Write as _;

    let directory = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(directory)?;
    let bytes = serde_json::to_vec_pretty(cache).map_err(std::io::Error::other)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    crate::private_file::set_owner_only(temporary.path())?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(file).map_err(|error| error.error)?;
    Ok(())
}

/// The cache file of this process and the tool paths it holds.
struct Active {
    file: PathBuf,
    tools: Mutex<BTreeMap<String, PathBuf>>,
}

static ACTIVE: OnceLock<Active> = OnceLock::new();

/// Use `file` for tool lookups in this process from now on.
pub(super) fn activate(file: PathBuf, cache: Option<&PathCache>) {
    let tools = cache.map(|cache| cache.tools.clone()).unwrap_or_default();
    let _ = ACTIVE.set(Active {
        file,
        tools: Mutex::new(tools),
    });
}

/// Save `found` for `name` when it differs from the saved path. Only a file
/// that [`super::apply`] saved gets tool paths; errors are ignored.
pub(super) fn remember_tool(name: &str, found: &Path) {
    let Some(active) = ACTIVE.get() else {
        return;
    };
    let Ok(mut tools) = active.tools.lock() else {
        return;
    };
    if tools.get(name).is_some_and(|saved| saved == found) {
        return;
    }
    if remember_in(&active.file, name, found).is_ok() {
        tools.insert(name.to_string(), found.to_path_buf());
    }
}

/// Re-read `file`, set the tool path and write it back.
pub(super) fn remember_in(file: &Path, name: &str, found: &Path) -> std::io::Result<()> {
    let Some(mut cache) = read_cache(file) else {
        return Err(std::io::Error::other("no path cache"));
    };
    cache.tools.insert(name.to_string(), found.to_path_buf());
    write_cache(file, &cache)
}

/// The saved path of `name` in `cache`, with whether `usable` accepts it.
pub(super) fn cached_tool_in(
    cache: &PathCache,
    name: &str,
    usable: fn(&Path) -> bool,
) -> Option<CachedTool> {
    let path = cache.tools.get(name)?;
    Some(CachedTool {
        path: path.clone(),
        saved_at: cache.saved_at,
        usable: usable(path),
    })
}

/// The saved path of `name` when it is still usable; a use is logged.
pub(super) fn cached_tool(name: &str) -> Option<PathBuf> {
    let active = ACTIVE.get()?;
    let tool = read_cached_tool(&active.file, name)?;
    record_tool_use(name, tool)
}

fn record_tool_use(name: &str, tool: CachedTool) -> Option<PathBuf> {
    if !tool.usable {
        return None;
    }
    let path = tool.path.display().to_string();
    Logger::new("path").record(
        Level::Info,
        "cached tool path used",
        &[("tool", name), ("path", &path)],
    );
    Some(tool.path)
}

/// Read the active cached tool without recording or logging its use.
pub(super) fn cached_tool_readonly(name: &str) -> Option<PathBuf> {
    let active = ACTIVE.get()?;
    cached_tool_readonly_in(&active.file, name)
}

pub(super) fn cached_tool_readonly_in(file: &Path, name: &str) -> Option<PathBuf> {
    let tool = read_cached_tool(file, name)?;
    tool.usable.then_some(tool.path)
}

fn read_cached_tool(file: &Path, name: &str) -> Option<CachedTool> {
    let cache = read_cache(file)?;
    cached_tool_in(&cache, name, super::lookup::usable_cached)
}

#[cfg(test)]
mod tests;
