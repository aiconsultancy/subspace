//! Canonical MXC Seatbelt worker for experimental Board Lab.
//!
//! Supported only on macOS aarch64. Other hosts return a clear error and never
//! run the command unsandboxed.

use std::path::{Path, PathBuf};

use anyhow::Result;

mod persistent;
mod platform;
mod selected_environment;

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
mod env;
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
mod output;
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
mod paths;
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
mod probe_script;
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
mod quote;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod sandbox;
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod unsupported;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use sandbox as backend;
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
use unsupported as backend;

pub use persistent::{PersistentProcess, SpawnRequest, spawn};
pub use platform::supports_platform;
pub use selected_environment::validate_environment_name;

pub fn capabilities() -> String {
    backend::capabilities()
}

// qual:api - Board Lab owns live contained execution.
pub fn run(
    executable: &Path,
    args: &[&str],
    readonly_root: &Path,
    workspace: &Path,
) -> Result<String> {
    backend::run(executable, args, readonly_root, workspace)
}

// qual:api - Board Lab owns live contained execution.
pub fn run_scoped(
    executable: &Path,
    args: &[&str],
    readonly_roots: &[PathBuf],
    workspace: &Path,
    proxy: Option<&str>,
) -> Result<String> {
    backend::run_scoped(executable, args, readonly_roots, workspace, proxy)
}

// qual:api - Board Lab owns the live containment probe.
pub fn probe(executable: &Path, readonly_root: &Path, workspace: &Path) -> Result<String> {
    backend::probe(executable, readonly_root, workspace)
}

#[cfg(test)]
mod tests {
    use super::capabilities;
    use super::supports_platform;

    #[test]
    fn capabilities_returns_a_platform_string() {
        assert!(!capabilities().is_empty());
    }

    #[test]
    fn public_supports_platform_matches_macos_aarch64_only() {
        assert!(supports_platform("macos", "aarch64"));
        assert!(!supports_platform("linux", "aarch64"));
    }
}
