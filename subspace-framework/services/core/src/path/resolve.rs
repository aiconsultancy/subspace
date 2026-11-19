//! Resolving the `PATH` once at startup and applying it to this process.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};

use super::cache::{CachedPath, PathCache, read_cache};
use super::capture::{Capture, joined};
use super::entries::{distinct, merge, missing_variables};
use super::fallback::{self, Probe, env_var};
use super::{DISABLE_ENV, DISABLE_ENV_ALIAS, save, shell_timeout};

/// Where the effective `PATH` came from, for display and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The login shell at this path printed its `PATH`.
    LoginShell(PathBuf),
    /// The machine and user `Path` registry values (Windows).
    Registry,
    /// The inherited `PATH` was kept, with the reason.
    Inherited(String),
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Source::LoginShell(shell) => format!("login shell {}", shell.display()),
            Source::Registry => "Windows registry".to_string(),
            Source::Inherited(reason) => format!("inherited ({reason})"),
        }
    }
}

/// A failed capture: what happened, and the reason text for
/// [`Source::Inherited`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptureError {
    pub(super) capture: Capture,
    pub(super) message: String,
}

/// What the login shell or the registry reported.
pub(super) struct Found {
    pub(super) path: OsString,
    pub(super) source: Source,
    pub(super) capture: Capture,
    pub(super) network: Vec<(String, String)>,
}

/// The outcome of [`apply`]: the merged `PATH` and how it was made.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub path: OsString,
    pub source: Source,
    /// Whether the login shell or the registry was asked, and the result.
    pub capture: Capture,
    /// Entries the capture, the saved `PATH` and the fallback folders added
    /// to the inherited `PATH`.
    pub added: usize,
    /// The usual install folders ([`super::FALLBACK_DIRECTORIES`]) appended to
    /// `path`, in order.
    pub fallback: Vec<PathBuf>,
    /// The saved `PATH`, when the shell failed and it was used.
    pub cached: Option<CachedPath>,
    /// Whether `path` differs from the inherited `PATH`, by new entries or
    /// by order.
    pub changed: bool,
    /// Network variables the login shell sets and this process lacks.
    pub network: Vec<(String, String)>,
}

impl Resolved {
    fn inherited_path(path: OsString, reason: &str, capture: Capture) -> Self {
        Self {
            path,
            source: Source::Inherited(reason.to_string()),
            capture,
            added: 0,
            fallback: Vec::new(),
            cached: None,
            changed: false,
            network: Vec::new(),
        }
    }

    fn merged(path: OsString, found: Found, current: &OsString) -> Self {
        Self {
            added: distinct(&path).len() - distinct(current).len(),
            changed: path != *current,
            path,
            source: found.source,
            capture: found.capture,
            fallback: Vec::new(),
            cached: None,
            network: missing_variables(found.network, |name| std::env::var_os(name).is_some()),
        }
    }

    /// Append `directories` to `path`, and measure `added` and `changed`
    /// against the inherited `current` again.
    fn append(&mut self, directories: &[PathBuf], current: &OsString) {
        if directories.is_empty() {
            return;
        }
        let mut entries = distinct(&self.path);
        entries.extend(directories.iter().cloned());
        if let Ok(path) = std::env::join_paths(&entries) {
            self.path = path;
            self.added = distinct(&self.path)
                .len()
                .saturating_sub(distinct(current).len());
            self.changed = self.path != *current;
        }
    }

    /// Append the saved folders of `cache` that still exist.
    fn use_cache(&mut self, cache: &PathCache, probe: &Probe<'_>, current: &OsString) {
        let directories =
            fallback::missing_directories(&distinct(&self.path), cache.path.clone(), probe);
        self.append(&directories, current);
        self.cached = Some(CachedPath {
            saved_at: cache.saved_at,
            source: cache.source.clone(),
            version: cache.version.clone(),
            added: directories.len(),
        });
    }

    /// Whether the login shell or the registry was asked and failed.
    pub fn capture_failed(&self) -> bool {
        self.capture.failed()
    }

    /// What supplied the `PATH`: `login_shell`, `registry`, `terminal`,
    /// `disabled`, or after a failed capture `cache` when saved folders were
    /// added, else `fallback`.
    pub fn path_from(&self) -> &'static str {
        match &self.capture {
            Capture::Captured { .. } => "login_shell",
            Capture::Registry => "registry",
            Capture::Terminal => "terminal",
            Capture::Disabled => "disabled",
            _ if self.cached.as_ref().is_some_and(|cached| cached.added > 0) => "cache",
            _ => "fallback",
        }
    }

    /// The fields of the `startup path` log record at `now`. `cache_age` is
    /// whole seconds since the saved `PATH` was written, or `-`. The fields
    /// name sources and appended folders and count entries and variables;
    /// they never hold the `PATH` value or an environment variable, because
    /// proxy variables can hold credentials.
    pub fn log_fields(&self, now: DateTime<Utc>) -> Vec<(&'static str, String)> {
        let cache_age = self.cached.as_ref().map_or_else(
            || "-".to_string(),
            |cached| (now - cached.saved_at).num_seconds().max(0).to_string(),
        );
        vec![
            ("source", self.source.describe()),
            ("capture", self.capture.label().to_string()),
            (
                "capture_seconds",
                self.capture.seconds().unwrap_or_else(|| "-".to_string()),
            ),
            ("path_from", self.path_from().to_string()),
            ("cache_age", cache_age),
            ("added", self.added.to_string()),
            ("fallback", joined(&self.fallback)),
            ("changed", self.changed.to_string()),
            ("network_variables", self.network.len().to_string()),
        ]
    }
}

/// What the host passes to [`apply`].
#[derive(Debug, Clone)]
pub struct Startup {
    /// `<state dir>/path-cache.json` of the running product, or `None` to
    /// neither read nor save a cache.
    pub cache_file: Option<PathBuf>,
    /// Ask the login shell even when started from a terminal.
    pub force: bool,
}

static APPLIED: OnceLock<Resolved> = OnceLock::new();

/// What [`apply`] set, if it ran.
pub fn applied() -> Option<&'static Resolved> {
    APPLIED.get()
}

static SEARCH_FOLDERS: OnceLock<Vec<PathBuf>> = OnceLock::new();

/// The fallback folders `find_tool` searches, or `None` before [`apply`]
/// ran. Tests that never apply the startup `PATH` therefore never search
/// this computer's install folders.
pub(super) fn search_folders() -> Option<&'static [PathBuf]> {
    SEARCH_FOLDERS.get().map(Vec::as_slice)
}

/// Resolve the full `PATH`, set it and the missing network variables for
/// this process, and save the `PATH` when it is known good.
///
/// # Safety
///
/// Call this before the process starts any other thread: it changes the
/// process environment with `std::env::set_var`. The login-shell capture
/// starts one reader thread. It joins that thread before it returns, also
/// after a timeout, once the killed process group has closed the pipe. When
/// a descendant left the group and still holds the pipe, the reader can
/// still run; `apply` then changes no variable at all, so no other thread
/// can observe a change.
// qual:allow(complexity, unsafe) reason: "The host calls this before app threads exist; the shell reader is joined first, and when it cannot be joined the environment is left unchanged; Rust 2024 set_var is unsafe and child-only env does not update this process."
pub unsafe fn apply(startup: Startup) -> &'static Resolved {
    let cache = startup.cache_file.as_deref().and_then(read_cache);
    let folders = fallback::startup_folders(&Probe::from_env(&env_var));
    let resolved = resolve(startup.force, cache.as_ref(), folders.clone());
    // SAFETY: the caller starts no thread before this; `reader_running`
    // tells whether the shell reader could not be joined.
    unsafe { set_environment(&resolved, reader_still_running()) };
    let _ = SEARCH_FOLDERS.set(folders);
    save::save(startup.cache_file, &resolved, cache, Utc::now());
    APPLIED.get_or_init(|| resolved)
}

/// Set `PATH` and the missing network variables, unless the shell reader
/// still runs (`reader_running`); then change nothing.
///
/// # Safety
///
/// No other thread may exist, apart from a reader that `reader_running`
/// reports, which this function then leaves alone.
// qual:allow(unsafe) reason: "Only apply calls this, before any thread exists and after the shell reader was joined; with a running reader nothing is changed."
unsafe fn set_environment(resolved: &Resolved, reader_running: bool) {
    if reader_running {
        return;
    }
    if resolved.changed {
        // SAFETY: no other thread exists: the caller starts none, and the
        // shell reader has been joined.
        unsafe { std::env::set_var("PATH", &resolved.path) };
    }
    for (name, value) in &resolved.network {
        // SAFETY: as above.
        unsafe { std::env::set_var(name, value) };
    }
}

/// Work out the `PATH` a terminal would have. Without `force`, a process that
/// runs in a terminal keeps its `PATH`, because it already has the shell's.
fn resolve(force: bool, cache: Option<&PathCache>, folders: Vec<PathBuf>) -> Resolved {
    resolve_with(Inputs {
        current: std::env::var_os("PATH").unwrap_or_default(),
        disabled: disabled_by(&env_var),
        terminal: terminal_start(force),
        capture: platform_path,
        cache,
        fallback: folders,
        probe: Probe::from_env(&env_var),
        inherited_first: cfg!(windows),
    })
}

/// The variable that turns the capture off, when one is `0`.
pub(super) fn disabled_by(lookup: &dyn Fn(&str) -> Option<String>) -> Option<&'static str> {
    [DISABLE_ENV, DISABLE_ENV_ALIAS]
        .into_iter()
        .find(|name| lookup(name).as_deref() == Some("0"))
}

/// What [`resolve_with`] reads from the process and the file system.
pub(super) struct Inputs<'a, C> {
    pub(super) current: OsString,
    pub(super) disabled: Option<&'static str>,
    pub(super) terminal: bool,
    pub(super) capture: C,
    pub(super) cache: Option<&'a PathCache>,
    /// The expanded fallback folders, existing or not.
    pub(super) fallback: Vec<PathBuf>,
    pub(super) probe: Probe<'a>,
    /// Merge order of a successful capture: the inherited `PATH` first
    /// (Windows registry), or the captured one first (login shell).
    pub(super) inherited_first: bool,
}

/// A disabled or terminal resolution keeps `current` unchanged. Otherwise
/// the capture leads (see [`captured`]). After a failed capture the saved
/// folders that still exist follow; the fallback folders come last.
pub(super) fn resolve_with<C>(inputs: Inputs<'_, C>) -> Resolved
where
    C: FnOnce(Duration) -> Result<Found, CaptureError>,
{
    let current = inputs.current;
    if let Some(name) = inputs.disabled {
        return Resolved::inherited_path(current, &format!("{name}=0"), Capture::Disabled);
    }
    if inputs.terminal {
        return Resolved::inherited_path(current, "started from a terminal", Capture::Terminal);
    }
    let timeout = shell_timeout(inputs.cache.is_some());
    let mut resolved = captured(
        (inputs.capture)(timeout),
        current.clone(),
        inputs.inherited_first,
    );
    if resolved.capture_failed()
        && let Some(cache) = inputs.cache
    {
        resolved.use_cache(cache, &inputs.probe, &current);
    }
    let directories =
        fallback::missing_directories(&distinct(&resolved.path), inputs.fallback, &inputs.probe);
    resolved.append(&directories, &current);
    resolved.fallback = directories;
    resolved
}

/// The `PATH` from a capture `outcome`, or `current` with the failure.
fn captured(
    outcome: Result<Found, CaptureError>,
    current: OsString,
    inherited_first: bool,
) -> Resolved {
    match outcome {
        Ok(found) => {
            // The login shell defines what a terminal has, so it leads. The
            // registry only holds machine and user values, so on Windows the
            // inherited PATH leads and keeps any directories a terminal or a
            // launcher added for this session; the registry adds what is
            // missing, such as a tool installed after sign-in.
            let path = if inherited_first {
                merge(&current, &found.path)
            } else {
                merge(&found.path, &current)
            };
            Resolved::merged(path, found, &current)
        }
        Err(error) => Resolved::inherited_path(current, &error.message, error.capture),
    }
}

#[cfg(unix)]
use super::shell::{platform_path, reader_still_running, terminal_start};

#[cfg(windows)]
use super::registry::platform_path;

/// Windows needs no terminal check: the inherited PATH always leads there, so
/// a terminal launch keeps its order and only gains missing registry entries.
#[cfg(not(unix))]
fn terminal_start(_force: bool) -> bool {
    false
}

/// Only the Unix login-shell capture starts a reader thread.
#[cfg(not(unix))]
fn reader_still_running() -> bool {
    false
}

/// Other platforms have no login shell or registry to ask.
#[cfg(not(any(unix, windows)))]
fn platform_path(_timeout: Duration) -> Result<Found, CaptureError> {
    Err(CaptureError {
        capture: Capture::Other {
            reason: "unsupported platform".to_string(),
        },
        message: "unsupported platform".to_string(),
    })
}

#[cfg(test)]
mod log_tests;
#[cfg(test)]
mod tests;
