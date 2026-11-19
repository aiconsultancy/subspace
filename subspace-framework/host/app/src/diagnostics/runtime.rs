//! Local-only facts. Collection runs before console detachment, never in render.

#[cfg(unix)]
use std::io::IsTerminal as _;

use subspace_core::diagnostics::{LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails};
use subspace_core::path::{self, Capture, Resolved};
use subspace_product::ProductContext;

pub(crate) fn local_copy(context: &ProductContext) -> LocalDiagnostics {
    let runtime = RuntimeDetails::new(executable(), launch_kind(), path_source(), azure_cli());
    let redaction = RedactionContext::new(
        Some(context.env.home.to_string_lossy().into_owned()),
        std::env::vars_os().map(|(_, value)| value.to_string_lossy().into_owned()),
    );
    LocalDiagnostics::new(super::build_identity(context), &context.subspace_version)
        .with_os_version(sysinfo::System::os_version().unwrap_or_else(|_| "unknown".into()))
        .with_runtime(runtime)
        .with_redaction(redaction)
}

fn executable() -> String {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".into())
}

fn path_source() -> String {
    path::applied().map_or_else(
        || "inherited (login PATH resolution did not run)".into(),
        source_description,
    )
}

fn source_description(resolved: &Resolved) -> String {
    match &resolved.capture {
        Capture::Disabled => "inherited (login-shell PATH resolution disabled)".into(),
        Capture::Terminal => "inherited (started from a terminal)".into(),
        Capture::Captured { .. } | Capture::Registry => resolved.source.describe(),
        capture => format!(
            "inherited ({}; capture {})",
            resolved.path_from(),
            capture.label()
        ),
    }
}

fn azure_cli() -> Option<String> {
    // Filesystem lookup only. Do not start az, a shell or another process.
    path::find_tool_readonly("az")
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

#[cfg(unix)]
fn launch_kind() -> LaunchKind {
    terminal_handles(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        std::io::stderr().is_terminal(),
        std::fs::File::open("/dev/tty").is_ok_and(|file| file.is_terminal()),
    )
}

#[cfg(windows)]
// qual:allow(unsafe) reason: "The console API writes to one valid PID slot; only the returned count is retained."
fn launch_kind() -> LaunchKind {
    let mut process = 0;
    // SAFETY: process is a valid writable u32 for this call. A desktop-launched
    // console-subsystem binary owns a new console alone; a terminal launch
    // shares the parent's console. Sample before FreeConsole detaches us.
    let count =
        unsafe { windows_sys::Win32::System::Console::GetConsoleProcessList(&mut process, 1) };
    terminal_handles(false, false, false, count > 1)
}

/// Redirected streams can still have a controlling terminal. No environment
/// value is evidence of a terminal: desktop launchers may inherit TERM too.
fn terminal_handles(stdin: bool, stdout: bool, stderr: bool, controlling: bool) -> LaunchKind {
    if stdin || stdout || stderr || controlling {
        LaunchKind::Terminal
    } else {
        LaunchKind::Desktop
    }
}

#[cfg(test)]
mod tests {
    use super::{source_description, terminal_handles};
    use subspace_core::diagnostics::LaunchKind;
    use subspace_core::path::{CacheSource, CachedPath, Capture, Resolved, Source};

    #[test]
    fn desktop_and_terminal_launches_include_redirected_streams() {
        assert_eq!(
            terminal_handles(false, false, false, false),
            LaunchKind::Desktop
        );
        for handles in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert_eq!(
                terminal_handles(handles.0, handles.1, handles.2, false),
                LaunchKind::Terminal
            );
        }
        assert_eq!(
            terminal_handles(false, false, false, true),
            LaunchKind::Terminal
        );
    }

    #[test]
    fn disabled_path_resolution_keeps_its_reason_without_an_environment_value() {
        for variable in [
            subspace_core::path::DISABLE_ENV,
            subspace_core::path::DISABLE_ENV_ALIAS,
        ] {
            let mut resolved = fixture(Capture::Disabled);
            resolved.source = Source::Inherited(format!("{variable}=0"));
            assert_eq!(
                source_description(&resolved),
                "inherited (login-shell PATH resolution disabled)"
            );
        }
    }

    fn fixture(capture: Capture) -> Resolved {
        Resolved {
            path: "/fixture/private/path".into(),
            source: Source::Inherited("private reason".into()),
            capture,
            added: 0,
            fallback: Vec::new(),
            cached: None,
            changed: false,
            network: Vec::new(),
        }
    }

    #[test]
    fn path_provenance_uses_typed_capture_and_effective_source() {
        let shell = std::path::PathBuf::from("/fixture/bin/zsh");
        let mut captured = fixture(Capture::Captured {
            shell: shell.clone(),
            elapsed_ms: 10,
        });
        captured.source = Source::LoginShell(shell.clone());
        assert_eq!(
            source_description(&captured),
            "login shell /fixture/bin/zsh"
        );
        assert_eq!(
            source_description(&fixture(Capture::Terminal)),
            "inherited (started from a terminal)"
        );
        let mut failed = fixture(Capture::Timeout {
            shell,
            limit_ms: 1000,
        });
        assert_eq!(
            source_description(&failed),
            "inherited (fallback; capture timeout)"
        );
        failed.cached = Some(CachedPath {
            saved_at: chrono::Utc::now(),
            source: CacheSource::Terminal,
            version: "1".into(),
            added: 1,
        });
        assert_eq!(
            source_description(&failed),
            "inherited (cache; capture timeout)"
        );
    }
}
