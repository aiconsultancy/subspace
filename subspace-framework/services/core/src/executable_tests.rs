//! Executable test fixtures that are safe to run while other tests fork.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

/// Writes an executable fixture without this process ever holding a write
/// handle to it. A test that forks concurrently can inherit an open write
/// handle until its child execs, and running the file meanwhile fails with
/// `ETXTBSY` ("Text file busy"). The text goes to a separate source file and a
/// short-lived `cp` child creates the executable, so only that child opens it
/// for writing, and it has exited before the fixture runs.
// qual:allow(dry, duplicate) reason: "Test fixture shared with subspace-agent; the crates have no common test-support crate."
pub(crate) fn write_executable(path: &Path, text: &str) {
    let source = path.with_extension("source");
    fs::write(&source, text).unwrap();
    let copied = std::process::Command::new("cp")
        .arg(&source)
        .arg(path)
        .status()
        .unwrap();
    assert!(copied.success(), "cannot create fixture {}", path.display());
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
