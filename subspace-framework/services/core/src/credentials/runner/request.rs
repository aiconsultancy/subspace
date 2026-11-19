//! Process request and captured-response records with redacted Debug output.

use std::time::Duration;

/// A command to run. Debug omits arguments and environment.
pub(super) struct RunnerRequest<'a> {
    pub(super) program: &'a str,
    pub(super) args: &'a [&'a str],
    pub(super) remove: &'a [&'a str],
    pub(super) timeout: Duration,
}

impl<'a> RunnerRequest<'a> {
    pub(super) fn new(
        program: &'a str,
        args: &'a [&'a str],
        remove: &'a [&'a str],
        timeout: Duration,
    ) -> Self {
        Self {
            program,
            args,
            remove,
            timeout,
        }
    }
}

impl std::fmt::Debug for RunnerRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerRequest")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// Captured stdout and exit code, including nonzero exits. Debug redacts stdout.
pub(super) struct CapturedOutput {
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
}

impl CapturedOutput {
    pub(super) fn new(exit_code: Option<i32>, stdout: String) -> Self {
        Self { exit_code, stdout }
    }
}

impl std::fmt::Debug for CapturedOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedOutput")
            .field("exit_code", &self.exit_code)
            .finish_non_exhaustive()
    }
}
