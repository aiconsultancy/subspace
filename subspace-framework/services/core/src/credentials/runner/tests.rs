use std::time::{Duration, Instant};

use super::{CapturedOutput, Runner, RunnerRequest, SystemRunner};

#[test]
fn request_and_output_debug_never_expose_arguments_or_output() {
    let args = ["test-only-private-request"];
    let request = RunnerRequest::new("test-only-program", &args, &[], Duration::from_secs(1));
    assert!(!format!("{request:?}").contains("test-only"));
    let output = CapturedOutput::new(Some(1), "test-only-private-response".into());
    assert!(!format!("{output:?}").contains("test-only"));
    assert_eq!(output.exit_code, Some(1));
}

/// The program is created by `write_executable`, so this process never holds
/// a write handle that a concurrent fork could keep open (Linux `ETXTBSY`).
#[cfg(unix)]
fn script(body: &str) -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test-only-runner");
    crate::executable_tests::write_executable(&path, &format!("#!/bin/sh\n{body}\n"));
    (directory, path.to_str().unwrap().to_owned())
}

#[test]
#[cfg(unix)]
fn system_runner_drains_full_stdout_and_stderr_and_keeps_exit() {
    let (_directory, program) =
        script("head -c 262144 /dev/zero\nhead -c 262144 /dev/zero >&2\nexit 1");
    let request = RunnerRequest::new(&program, &[], &[], Duration::from_secs(5));
    let output = super::process::capture(request).unwrap();
    assert_eq!(output.exit_code, Some(1));
    assert_eq!(output.stdout.len(), 262144);
}

#[test]
#[cfg(unix)]
fn system_runner_stops_a_program_at_the_timeout() {
    let (_directory, program) = script("exec sleep 10");
    let started = Instant::now();
    let error = SystemRunner
        .run(&program, &[], &[], Duration::from_millis(100))
        .unwrap_err();
    assert!(error.contains("timeout"), "not a timeout: {error}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
#[cfg(unix)]
fn system_runner_failure_errors_never_repeat_stdout_or_stderr() {
    let (_directory, program) =
        script("echo test-only-private-stdout\necho test-only-private-stderr >&2\nexit 1");
    let error = SystemRunner
        .run(&program, &[], &[], Duration::from_secs(1))
        .unwrap_err();
    assert!(!error.contains("test-only-private"));
}
