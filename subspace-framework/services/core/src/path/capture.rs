//! What the login-shell capture did, and its product-neutral reason text.
//! Nothing here holds a `PATH` value or an environment variable value.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The result of asking the login shell (or, on Windows, the registry) for
/// the `PATH` a terminal has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Capture {
    /// The login shell printed its PATH.
    Captured { shell: PathBuf, elapsed_ms: u64 },
    /// The login shell did not print its PATH within the limit.
    Timeout { shell: PathBuf, limit_ms: u64 },
    /// The login shell started but printed no PATH between the markers.
    NoPath { shell: PathBuf, elapsed_ms: u64 },
    /// The login shell could not start; `error` is the io::ErrorKind text
    /// only (e.g. "entity not found"), never paths or env.
    StartFailed { shell: PathBuf, error: String },
    /// Not attempted: started from a terminal.
    Terminal,
    /// Not attempted: SUBSPACE_GUI_LOGIN_PATH=0.
    Disabled,
    /// Windows: the registry supplied Path.
    Registry,
    /// Any other failure (Windows registry errors), reason text without env
    /// values.
    Other { reason: String },
}

const MILLISECONDS_PER_SECOND: u64 = 1000;

/// `3.4` for 3 449 ms: the log and Details precision.
pub(super) fn seconds(milliseconds: u64) -> String {
    format!(
        "{:.1}",
        milliseconds as f64 / MILLISECONDS_PER_SECOND as f64
    )
}

/// `10` for 10 000 ms, `2.5` for 2 500 ms: a limit as people write it.
pub(super) fn limit_seconds(milliseconds: u64) -> String {
    if milliseconds.is_multiple_of(MILLISECONDS_PER_SECOND) {
        (milliseconds / MILLISECONDS_PER_SECOND).to_string()
    } else {
        seconds(milliseconds)
    }
}

impl Capture {
    /// The snake_case kind, as serialized and as the `capture` log field.
    pub fn label(&self) -> &'static str {
        match self {
            Capture::Captured { .. } => "captured",
            Capture::Timeout { .. } => "timeout",
            Capture::NoPath { .. } => "no_path",
            Capture::StartFailed { .. } => "start_failed",
            Capture::Terminal => "terminal",
            Capture::Disabled => "disabled",
            Capture::Registry => "registry",
            Capture::Other { .. } => "other",
        }
    }

    /// Whether the login shell or the registry was asked and failed.
    pub fn failed(&self) -> bool {
        matches!(
            self,
            Capture::Timeout { .. }
                | Capture::NoPath { .. }
                | Capture::StartFailed { .. }
                | Capture::Other { .. }
        )
    }

    /// The `capture_seconds` log value: the elapsed time, or the limit for
    /// a timeout, to 0.1 s; `None` when nothing ran or nothing was timed.
    pub fn seconds(&self) -> Option<String> {
        match self {
            Capture::Captured { elapsed_ms, .. } | Capture::NoPath { elapsed_ms, .. } => {
                Some(seconds(*elapsed_ms))
            }
            Capture::Timeout { limit_ms, .. } => Some(seconds(*limit_ms)),
            _ => None,
        }
    }

    /// Why the app could not read the shell's `PATH`, to follow the app
    /// name: "could not read your PATH: /bin/zsh did not finish starting
    /// within 10 s". `None` when the `PATH` was read or not asked for.
    pub fn reason(&self) -> Option<String> {
        let detail = match self {
            Capture::Timeout { shell, limit_ms } => format!(
                "{} did not finish starting within {} s",
                shell.display(),
                limit_seconds(*limit_ms)
            ),
            Capture::NoPath { shell, .. } => {
                format!("{} started but printed no PATH", shell.display())
            }
            Capture::StartFailed { shell, error } => {
                format!("{} could not start: {error}", shell.display())
            }
            Capture::Other { reason } => reason.clone(),
            _ => return None,
        };
        Some(format!("could not read your PATH: {detail}"))
    }
}

/// `a, b`: directories as one log or Details value.
pub(super) fn joined(directories: &[PathBuf]) -> String {
    directories
        .iter()
        .map(|directory| directory.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
