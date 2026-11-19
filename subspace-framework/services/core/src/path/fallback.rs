//! The usual install folders appended after the captured or inherited
//! `PATH`, so tools such as `az` are found when the login shell is slow or
//! prints nothing. The user's own `PATH` order always comes first. System
//! folders come before per-user ones. A folder is appended only when it
//! exists and `PATH` does not already hold it.
//!
//! Each list is a set of templates: `~/` starts at the home folder and
//! `%NAME%` reads an environment variable. A template whose home or variable
//! is not set is skipped, so no drive letter or user name is written here.
//! To add a folder, add one line with a comment that names what installs
//! there.

use std::path::{Path, PathBuf};

use super::registry::expand_percent;

/// macOS install folders.
pub const MACOS_DIRECTORIES: &[&str] = &[
    // Homebrew on Apple silicon: the azure-cli formula and cask.
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    // Homebrew on Intel; Docker Desktop's kubectl.
    "/usr/local/bin",
    "/usr/local/sbin",
    // MacPorts.
    "/opt/local/bin",
    // Microsoft's Azure CLI install script (default exec folder).
    "~/bin",
    // pipx and pip --user.
    "~/.local/bin",
];

/// Linux install folders. The deb and rpm packages put `az` in `/usr/bin`,
/// which every `PATH` already holds.
pub const LINUX_DIRECTORIES: &[&str] = &[
    // Manual installs and many vendor packages.
    "/usr/local/bin",
    // Snap packages.
    "/snap/bin",
    // Homebrew on Linux, system and per-user prefixes.
    "/home/linuxbrew/.linuxbrew/bin",
    "~/.linuxbrew/bin",
    // Microsoft's Azure CLI install script (default exec folder).
    "~/bin",
    // pipx.
    "~/.local/bin",
];

/// Windows install folders. There is no `%LOCALAPPDATA%` entry: the Azure CLI
/// MSI installs per machine only, and the ZIP package goes to a folder the
/// user chooses, which no list can know.
pub const WINDOWS_DIRECTORIES: &[&str] = &[
    // 64-bit Azure CLI MSI; winget and Chocolatey use this MSI.
    r"%ProgramFiles%\Microsoft SDKs\Azure\CLI2\wbin",
    // 32-bit Azure CLI MSI.
    r"%ProgramFiles(x86)%\Microsoft SDKs\Azure\CLI2\wbin",
    // Chocolatey shims.
    r"%ProgramData%\chocolatey\bin",
    // Scoop shims; the Azure data client finds Scoop's Python beside them.
    r"%USERPROFILE%\scoop\shims",
    // pipx.
    r"%USERPROFILE%\.local\bin",
];

/// The list for this platform.
#[cfg(target_os = "macos")]
pub const FALLBACK_DIRECTORIES: &[&str] = MACOS_DIRECTORIES;
/// The list for this platform.
#[cfg(all(unix, not(target_os = "macos")))]
pub const FALLBACK_DIRECTORIES: &[&str] = LINUX_DIRECTORIES;
/// The list for this platform.
#[cfg(windows)]
pub const FALLBACK_DIRECTORIES: &[&str] = WINDOWS_DIRECTORIES;
/// No list for other platforms.
#[cfg(not(any(unix, windows)))]
pub const FALLBACK_DIRECTORIES: &[&str] = &[];

/// Debug builds only: a PATH-style list of folders that replaces the
/// platform list, for startup appending and for `find_tool`, so screenshots
/// and manual tests do not depend on what this computer has installed. An
/// empty value means no fallback folders. Release builds ignore it.
/// Example: `SUBSPACE_PATH_FALLBACK_FIXTURE=/tmp/fixture/bin:/tmp/fixture/sbin`.
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub(super) const FIXTURE_ENV: &str = "SUBSPACE_PATH_FALLBACK_FIXTURE";

/// The non-empty folders of a [`FIXTURE_ENV`] value, in order.
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub(super) fn parse_fixture(value: &std::ffi::OsStr) -> Vec<PathBuf> {
    std::env::split_paths(value)
        .filter(|entry| !entry.as_os_str().is_empty())
        .collect()
}

#[cfg(debug_assertions)]
fn fixture_folders() -> Option<Vec<PathBuf>> {
    std::env::var_os(FIXTURE_ENV).map(|value| parse_fixture(&value))
}

#[cfg(not(debug_assertions))]
fn fixture_folders() -> Option<Vec<PathBuf>> {
    None
}

/// The fallback folders of this process: the debug fixture when set, else
/// the platform list expanded with `probe`.
pub(super) fn startup_folders(probe: &Probe<'_>) -> Vec<PathBuf> {
    fixture_folders().unwrap_or_else(|| expand_all(FALLBACK_DIRECTORIES, probe))
}

/// What the fallback reads from the environment and the file system.
pub(super) struct Probe<'a> {
    /// The home folder for `~/`, or `None` when it is not set.
    pub(super) home: Option<PathBuf>,
    /// An environment variable for `%NAME%`.
    pub(super) lookup: &'a dyn Fn(&str) -> Option<String>,
    pub(super) is_directory: fn(&Path) -> bool,
    /// Compare folders as Windows does: `/` is `\` and case is ignored.
    pub(super) windows: bool,
}

/// The value of the environment variable `name`, if it is set.
pub(super) fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

impl<'a> Probe<'a> {
    /// The probe of this process: `HOME`, the real environment through
    /// `lookup` and the file system.
    pub(super) fn from_env(lookup: &'a dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            home: std::env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .map(PathBuf::from),
            lookup,
            is_directory: Path::is_dir,
            windows: cfg!(windows),
        }
    }
}

/// `template` with its home folder or variables filled in, or `None` when
/// one of them is not set.
pub(super) fn expand(
    template: &str,
    home: Option<&Path>,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    if let Some(rest) = template.strip_prefix("~/") {
        return home.map(|home| home.join(rest));
    }
    let expanded = expand_percent(template, |name| {
        lookup(name).filter(|value| !value.is_empty())
    });
    (!expanded.contains('%')).then(|| PathBuf::from(expanded))
}

/// `directory` as text that compares equal to the same folder written
/// another way: no trailing separator, and on Windows `\` for `/` and
/// lower case.
fn comparable(directory: &Path, windows: bool) -> String {
    let text = directory.to_string_lossy();
    let text = if windows {
        text.replace('/', "\\").to_lowercase()
    } else {
        text.into_owned()
    };
    let trimmed = text.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        text
    } else {
        trimmed.to_string()
    }
}

/// The `templates` with their home folder and variables filled in, in list
/// order; templates with an unset home or variable are left out.
pub(super) fn expand_all(templates: &[&str], probe: &Probe<'_>) -> Vec<PathBuf> {
    templates
        .iter()
        .filter_map(|template| expand(template, probe.home.as_deref(), probe.lookup))
        .collect()
}

/// The `candidates` that exist as folders and `present` lacks, in order,
/// without duplicates.
pub(super) fn missing_directories(
    present: &[PathBuf],
    candidates: Vec<PathBuf>,
    probe: &Probe<'_>,
) -> Vec<PathBuf> {
    let mut known: Vec<String> = present
        .iter()
        .map(|entry| comparable(entry, probe.windows))
        .collect();
    let mut missing = Vec::new();
    for directory in candidates {
        let key = comparable(&directory, probe.windows);
        if !known.contains(&key) && (probe.is_directory)(&directory) {
            known.push(key);
            missing.push(directory);
        }
    }
    missing
}

#[cfg(test)]
mod tests;
