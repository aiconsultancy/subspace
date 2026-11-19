//! Tool and `PATH` resolution for every Subspace app.
//!
//! A desktop app started from Finder, the Dock, a desktop entry or the Start
//! Menu does not inherit the `PATH` a terminal has. On macOS launchd gives it
//! `/usr/bin:/bin:/usr/sbin:/sbin`, so Azure CLI from Homebrew
//! (`/opt/homebrew/bin/az`) is not found. The host calls [`apply`] once at
//! startup, before any thread exists. It asks the user's login shell (on
//! Windows, the registry) for the `PATH` a terminal has and merges it with the
//! inherited one. When the shell fails, it adds the `PATH` that worked last
//! time ([`PathCache`]) and then the usual install folders of the platform
//! ([`FALLBACK_DIRECTORIES`]). Child processes inherit the result.
//!
//! The same login shell also supplies the proxy and certificate variables
//! ([`network`]) that this process lacks. They are never written to the cache.
//!
//! Every lookup of an external program uses [`find_tool`]: `PATH` first, then
//! the path where the tool was found last time. Do not add other `PATH`
//! lookups.

use std::path::{Path, PathBuf};
use std::time::Duration;

mod cache;
mod capture;
mod entries;
mod fallback;
mod lookup;
pub mod network;
#[cfg(unix)]
mod output;
mod registry;
mod resolve;
mod save;
#[cfg(unix)]
mod shell;

pub use cache::{CacheSource, CachedPath, CachedTool, PathCache, read_cache};
pub use capture::Capture;
pub use fallback::{
    FALLBACK_DIRECTORIES, LINUX_DIRECTORIES, MACOS_DIRECTORIES, WINDOWS_DIRECTORIES,
};
pub use lookup::{ToolError, find_tool, find_tool_readonly};
pub use resolve::{Resolved, Source, Startup, applied, apply};

/// Set to `0` to keep the inherited `PATH`.
pub const DISABLE_ENV: &str = "SUBSPACE_GUI_LOGIN_PATH";

/// The short name of [`DISABLE_ENV`]; either one set to `0` works.
pub const DISABLE_ENV_ALIAS: &str = "SUBSPACE_LOGIN_PATH";

/// How long the login shell may take to start and print its environment
/// when no saved `PATH` exists. This is the worst-case delay of a start: a
/// fast shell returns at the end marker, so only a slow or stuck shell waits
/// this long. Startup files that load nvm or endpoint-security agents can
/// take several seconds just after the computer starts. There is one
/// attempt, never a second one.
pub const SHELL_TIMEOUT: Duration = Duration::from_secs(10);

/// The shorter limit when a saved `PATH` can stand in for a slow shell.
pub const CACHED_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// The login-shell time limit: shorter when a saved `PATH` exists.
pub fn shell_timeout(cache_present: bool) -> Duration {
    if cache_present {
        CACHED_SHELL_TIMEOUT
    } else {
        SHELL_TIMEOUT
    }
}

/// The cache file name, beside the product's `state.json`.
pub const CACHE_FILE: &str = "path-cache.json";

/// `<state dir>/path-cache.json`.
pub fn cache_path(state_dir: &Path) -> PathBuf {
    state_dir.join(CACHE_FILE)
}

/// The markers around the `env` output of the login shell (Unix only).
#[cfg(unix)]
pub(crate) const START: &str = "__SUBSPACE_PATH_START__";
#[cfg(unix)]
pub(crate) const END: &str = "__SUBSPACE_PATH_END__";
