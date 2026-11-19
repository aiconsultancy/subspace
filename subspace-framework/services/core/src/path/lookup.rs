//! The one lookup of external programs.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::cache;
use super::resolve::search_folders;
use crate::log::{Level, Logger};

/// Unix execute bits (owner/group/other).
#[cfg(unix)]
const UNIX_EXECUTE_BITS: u32 = 0o111;

/// Windows extensions that start a program.
const WINDOWS_EXECUTABLE_EXTENSIONS: [&str; 4] = ["exe", "cmd", "bat", "com"];

/// Why [`find_tool`] found nothing. The texts match Go `exec.LookPath`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// A name with a path separator that is not an executable file.
    #[error("exec: {0:?}: not an executable file")]
    NotExecutable(String),
    /// No executable on `PATH` and no usable saved path.
    #[error("exec: {0:?}: executable file not found in $PATH")]
    NotFound(String),
}

/// Find an external program. A name with a path separator is used as given.
/// Otherwise the first executable match wins, in this order: each `PATH`
/// folder; the path where the tool was found last time, if it still exists
/// and is executable; the platform's usual install folders
/// ([`super::FALLBACK_DIRECTORIES`]) that exist. The last two steps run
/// only after [`super::apply`]: they use its cache file and its folders, so
/// a test that never applies the startup `PATH` never finds a tool outside
/// `PATH`. The last step also covers a start from a terminal whose `PATH`
/// lacks, for example, Homebrew. A `PATH` or install-folder result is saved,
/// as an absolute path, when it differs from the saved path; an
/// install-folder result is logged once per tool.
pub fn find_tool(name: &str) -> Result<PathBuf, ToolError> {
    separated(name).map_or_else(|| find_named(name), |path| given(path, name))
}

/// Resolve a tool using the same search order as [`find_tool`] without saving
/// paths or recording and logging cache or fallback use. No process is started.
pub fn find_tool_readonly(name: &str) -> Result<PathBuf, ToolError> {
    separated(name).map_or_else(|| find_named_readonly(name), |path| given(path, name))
}

/// `name` as a path when it holds a path separator.
fn separated(name: &str) -> Option<PathBuf> {
    (name.contains('/') || name.contains(std::path::MAIN_SEPARATOR)).then(|| PathBuf::from(name))
}

/// A bare `name` through [`locate`] with this process's environment.
fn find_named(name: &str) -> Result<PathBuf, ToolError> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let folders = search_folders().unwrap_or_default();
    locate(name, &path, folders, cfg!(windows))
        .map(|found| record(name, found))
        .ok_or_else(|| ToolError::NotFound(name.to_string()))
}

fn find_named_readonly(name: &str) -> Result<PathBuf, ToolError> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let folders = search_folders().unwrap_or_default();
    locate_with(
        name,
        &path,
        folders,
        cfg!(windows),
        cache::cached_tool_readonly,
    )
    .map(Located::into_path)
    .ok_or_else(|| ToolError::NotFound(name.to_string()))
}

/// Where [`locate`] found a tool.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Located {
    Path(PathBuf),
    Cached(PathBuf),
    Fallback(PathBuf),
}

impl Located {
    fn into_path(self) -> PathBuf {
        match self {
            Self::Path(path) | Self::Cached(path) | Self::Fallback(path) => path,
        }
    }
}

/// `name` on `path`, then at its saved path, then in the `fallback` folders.
pub(super) fn locate(
    name: &str,
    path: &OsStr,
    fallback: &[PathBuf],
    windows: bool,
) -> Option<Located> {
    locate_with(name, path, fallback, windows, cache::cached_tool)
}

fn locate_with(
    name: &str,
    path: &OsStr,
    fallback: &[PathBuf],
    windows: bool,
    cached: impl FnOnce(&str) -> Option<PathBuf>,
) -> Option<Located> {
    search(name, path)
        .map(Located::Path)
        .or_else(|| cached(name).map(Located::Cached))
        .or_else(|| in_fallback(name, fallback, windows).map(Located::Fallback))
}

/// The first executable `name` in the existing `fallback` folders.
pub(super) fn in_fallback(name: &str, fallback: &[PathBuf], windows: bool) -> Option<PathBuf> {
    fallback
        .iter()
        .filter(|directory| directory.is_dir())
        .flat_map(|directory| candidates(directory, name, windows))
        .find(|candidate| is_executable(candidate))
}

/// `path` as an absolute path from `directory`, the working directory of
/// the lookup. Symbolic links and `..` stay as they are.
pub(super) fn absolute_from(path: PathBuf, directory: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        directory.join(path)
    }
}

/// Save `found` for `name` as an absolute path.
fn remember(name: &str, found: &Path) {
    let directory = std::env::current_dir().unwrap_or_default();
    cache::remember_tool(name, &absolute_from(found.to_path_buf(), &directory));
}

static LOGGED_FALLBACK: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Save a `PATH` or install-folder result, log an install-folder result once,
/// and return the path.
fn record(name: &str, found: Located) -> PathBuf {
    match found {
        Located::Path(path) => {
            remember(name, &path);
            path
        }
        Located::Cached(path) => path,
        Located::Fallback(path) => {
            remember(name, &path);
            log_first_fallback(name, &path);
            path
        }
    }
}

/// Log the first install-folder result for `name` in this process.
fn log_first_fallback(name: &str, path: &Path) {
    let first = LOGGED_FALLBACK
        .lock()
        .is_ok_and(|mut logged| logged.insert(name.to_string()));
    if first {
        let text = path.display().to_string();
        Logger::new("path").record(
            Level::Info,
            "install folder tool path used",
            &[("tool", name), ("path", &text)],
        );
    }
}

fn given(path: PathBuf, name: &str) -> Result<PathBuf, ToolError> {
    Some(path)
        .filter(|path| is_executable(path))
        .ok_or_else(|| ToolError::NotExecutable(name.to_string()))
}

/// The first executable `name` in the folders of `path`.
pub(super) fn search(name: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .flat_map(|directory| candidates(&directory, name, cfg!(windows)))
        .find(|candidate| is_executable(candidate))
}

/// The files `name` can be in `directory`. On Windows, as Go
/// `exec.LookPath` does, a name without an executable extension is tried
/// only with one, so the extensionless `az` shell script beside `az.cmd`
/// is never chosen.
pub(super) fn candidates(directory: &Path, name: &str, windows: bool) -> Vec<PathBuf> {
    let base = directory.join(name);
    if !windows || has_executable_extension(&base) {
        return vec![base];
    }
    WINDOWS_EXECUTABLE_EXTENSIONS
        .iter()
        .map(|extension| directory.join(format!("{name}.{extension}")))
        .collect()
}

/// An existing file with an execute bit (Unix), or any existing file
/// (Windows, where the extension decides).
pub(super) fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & UNIX_EXECUTE_BITS != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Whether `path` has an extension that starts a Windows program.
pub(super) fn has_executable_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            WINDOWS_EXECUTABLE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// Whether a saved tool path can still run: executable, and on Windows with
/// an executable extension.
pub(super) fn usable_cached(path: &Path) -> bool {
    is_executable(path) && (!cfg!(windows) || has_executable_extension(path))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    #[cfg(unix)]
    use std::path::PathBuf;

    use super::super::resolve::search_folders;
    #[cfg(unix)]
    use super::{Located, absolute_from, search, usable_cached};
    use super::{
        ToolError, candidates, find_tool, find_tool_readonly, has_executable_extension, locate,
    };

    #[cfg(unix)]
    #[test]
    fn only_an_existing_executable_saved_path_is_usable() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().unwrap();
        let runnable = directory.path().join("az");
        crate::executable_tests::write_executable(&runnable, "#!/bin/sh\n");
        let plain = directory.path().join("plain");
        std::fs::write(&plain, "text").unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(usable_cached(&runnable));
        assert!(!usable_cached(&plain));
        assert!(!usable_cached(&directory.path().join("missing")));
        assert!(!usable_cached(directory.path()));
    }

    #[cfg(unix)]
    #[test]
    fn search_returns_the_first_executable_on_path() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::write(first.path().join("tool"), "not executable").unwrap();
        crate::executable_tests::write_executable(&second.path().join("tool"), "#!/bin/sh\n");
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();
        assert_eq!(search("tool", &path), Some(second.path().join("tool")));
        assert_eq!(search("absent", &path), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_tool_missing_from_a_terminal_path_is_found_in_an_install_folder() {
        use std::os::unix::fs::PermissionsExt as _;

        let terminal_path = tempfile::tempdir().unwrap();
        let install = tempfile::tempdir().unwrap();
        let folder = install.path().to_path_buf();
        let path = terminal_path.path().as_os_str();
        assert_eq!(
            locate("pi", path, std::slice::from_ref(&folder), false),
            None
        );
        let pi = install.path().join("pi");
        std::fs::write(&pi, "not executable").unwrap();
        std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            locate("pi", path, std::slice::from_ref(&folder), false),
            None
        );
        std::fs::remove_file(&pi).unwrap();
        crate::executable_tests::write_executable(&pi, "#!/bin/sh\n");
        let folders = [PathBuf::from("/fixture/missing/bin"), folder.clone()];
        assert_eq!(
            locate("pi", path, &folders, false),
            Some(Located::Fallback(pi.clone()))
        );
        crate::executable_tests::write_executable(&terminal_path.path().join("pi"), "#!/bin/sh\n");
        assert_eq!(
            locate("pi", path, &[folder], false),
            Some(Located::Path(terminal_path.path().join("pi")))
        );
    }

    #[test]
    fn without_the_startup_resolution_no_install_folder_is_searched() {
        // Tests never call `apply`, so `find_tool` gets no folders and finds
        // nothing outside PATH, whatever this computer has installed.
        assert_eq!(search_folders(), None);
        assert_eq!(
            locate(
                "subspace-fixture-absent-tool",
                std::ffi::OsStr::new(""),
                search_folders().unwrap_or_default(),
                false
            ),
            None
        );
        assert_eq!(
            find_tool("subspace-fixture-absent-tool"),
            Err(ToolError::NotFound("subspace-fixture-absent-tool".into()))
        );
        assert_eq!(
            find_tool_readonly("subspace-fixture-absent-tool"),
            Err(ToolError::NotFound("subspace-fixture-absent-tool".into()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn readonly_resolution_obeys_path_cache_fallback_order() {
        let path_dir = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        let cached = tempfile::tempdir().unwrap();
        let name = "readonly-order-fixture";
        let cached_tool = cached.path().join(name);
        let fallback_tool = fallback.path().join(name);
        crate::executable_tests::write_executable(&cached_tool, "#!/bin/sh\n");
        crate::executable_tests::write_executable(&fallback_tool, "#!/bin/sh\n");
        let folders = [fallback.path().to_path_buf()];
        let lookup = || {
            super::locate_with(name, path_dir.path().as_os_str(), &folders, false, |_| {
                Some(cached_tool.clone())
            })
            .map(Located::into_path)
        };
        assert_eq!(lookup(), Some(cached_tool.clone()));
        let path_tool = path_dir.path().join(name);
        crate::executable_tests::write_executable(&path_tool, "#!/bin/sh\n");
        assert_eq!(lookup(), Some(path_tool));
        std::fs::remove_file(path_dir.path().join(name)).unwrap();
        assert_eq!(
            super::locate_with(name, path_dir.path().as_os_str(), &folders, false, |_| None)
                .map(Located::into_path),
            Some(fallback_tool)
        );
    }

    #[cfg(unix)]
    #[test]
    fn tool_paths_are_saved_absolute_without_resolving_links() {
        let directory = Path::new("/fixture/work");
        assert_eq!(
            absolute_from(PathBuf::from("bin/az"), directory),
            PathBuf::from("/fixture/work/bin/az")
        );
        assert_eq!(
            absolute_from(PathBuf::from("../tools/az"), directory),
            PathBuf::from("/fixture/work/../tools/az")
        );
        assert_eq!(
            absolute_from(PathBuf::from("/opt/homebrew/bin/az"), directory),
            PathBuf::from("/opt/homebrew/bin/az")
        );
    }

    #[test]
    fn windows_names_get_an_executable_extension_unless_they_have_one() {
        let directory = Path::new("/fixture/bin");
        assert_eq!(
            candidates(directory, "az", true),
            ["az.exe", "az.cmd", "az.bat", "az.com"].map(|name| directory.join(name))
        );
        assert_eq!(
            candidates(directory, "Headlamp.exe", true),
            [directory.join("Headlamp.exe")]
        );
        assert_eq!(candidates(directory, "az", false), [directory.join("az")]);
    }

    #[test]
    fn windows_extensions_are_case_insensitive() {
        assert!(has_executable_extension(Path::new(r"C:\Tools\az.CMD")));
        assert!(has_executable_extension(Path::new("az.exe")));
        assert!(!has_executable_extension(Path::new("az")));
        assert!(!has_executable_extension(Path::new("az.ps1")));
    }

    #[test]
    fn errors_keep_the_go_texts() {
        assert_eq!(
            ToolError::NotFound("az".into()).to_string(),
            "exec: \"az\": executable file not found in $PATH"
        );
        assert_eq!(
            ToolError::NotExecutable("/x/az".into()).to_string(),
            "exec: \"/x/az\": not an executable file"
        );
    }
}
