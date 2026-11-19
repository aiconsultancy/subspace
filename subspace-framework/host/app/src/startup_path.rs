//! The `PATH` every app mode uses: applied once at the start of
//! [`crate::launch`], before any thread exists, and recorded in the log once
//! the log file is open.

use subspace_core::log::{Level, Logger};
use subspace_core::path::{self, Resolved, Startup};
use subspace_product::{PlatformEnv, ProductDescriptor};

/// Resolve and set the `PATH` for this process. The saved `PATH` lives
/// beside the product's `state.json`.
// qual:allow(unsafe) reason: "launch calls this first, before the desktop, CLI, MCP or service mode starts a thread; path::apply documents the requirement."
pub(crate) fn apply(descriptor: &ProductDescriptor) {
    let cache_file = PlatformEnv::from_process().ok().map(|env| {
        path::cache_path(&crate::product::state_directory(
            &env,
            descriptor.binary_name.as_str(),
        ))
    });
    let startup = Startup {
        cache_file,
        force: false,
    };
    // SAFETY: `launch` calls this before any other thread exists.
    unsafe {
        path::apply(startup);
    }
}

/// Write the `startup path` record, if [`apply`] ran.
pub(crate) fn record() {
    if let Some(resolved) = path::applied() {
        write(&Logger::new("path"), resolved);
    }
}

/// Warn for a failed capture, else info.
fn write(logger: &Logger, resolved: &Resolved) {
    let level = if resolved.capture_failed() {
        Level::Warn
    } else {
        Level::Info
    };
    let fields = resolved.log_fields(chrono::Utc::now());
    let fields = fields
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect::<Vec<_>>();
    logger.record(level, "startup path", &fields);
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use subspace_core::log::{Level, Logger, recent};
    use subspace_core::path::{Capture, Resolved, Source};

    use super::write;

    #[test]
    fn a_failed_capture_is_a_warning_without_path_or_variable_values() {
        let logger = Logger::new("path.startup-test");
        let resolved = Resolved {
            path: OsString::from("/fixture/secret/bin:/usr/bin:/opt/homebrew/bin"),
            source: Source::Inherited(
                "login shell /fixture/bin/zsh: no PATH within 10 seconds".into(),
            ),
            capture: Capture::Timeout {
                shell: PathBuf::from("/fixture/bin/zsh"),
                limit_ms: 10_000,
            },
            added: 1,
            fallback: vec![PathBuf::from("/opt/homebrew/bin")],
            cached: None,
            changed: true,
            network: vec![(
                "HTTPS_PROXY".into(),
                format!("http://{}:{}@proxy.fixture.invalid:8080", "user", "pw"),
            )],
        };
        write(&logger, &resolved);
        let written = recent()
            .into_iter()
            .rfind(|entry| entry.target == logger.target())
            .unwrap();
        assert_eq!(written.level, Level::Warn);
        assert_eq!(written.message, "startup path");
        let line = written.line();
        assert!(line.contains("capture=timeout"), "{line}");
        assert!(line.contains("capture_seconds=10.0"), "{line}");
        assert!(line.contains("path_from=fallback"), "{line}");
        assert!(line.contains("fallback=/opt/homebrew/bin"), "{line}");
        assert!(!line.contains("/fixture/secret/bin"), "{line}");
        assert!(!line.contains("proxy.fixture.invalid"), "{line}");
        assert!(!line.contains("HTTPS_PROXY"), "{line}");
    }
}
