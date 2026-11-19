use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::time::Duration;

use super::super::{CACHED_SHELL_TIMEOUT, Capture, END, SHELL_TIMEOUT, START, shell_timeout};
use super::{
    ShellCapture, ShellFailure, capture, login_capture, login_shell_with, query_account_shell,
    reader_still_running, shell_path,
};
use crate::executable_tests::write_executable;

/// A fake login shell running `body`, ignoring its arguments.
fn fake_shell(dir: &std::path::Path, body: &str) -> PathBuf {
    let shell = dir.join("fake-shell");
    write_executable(&shell, &format!("#!/bin/sh\n{body}\n"));
    shell
}

#[test]
fn the_limit_is_ten_seconds_or_five_with_a_saved_path() {
    assert_eq!(SHELL_TIMEOUT, Duration::from_secs(10));
    assert_eq!(CACHED_SHELL_TIMEOUT, Duration::from_secs(5));
    assert_eq!(shell_timeout(false), SHELL_TIMEOUT);
    assert_eq!(shell_timeout(true), CACHED_SHELL_TIMEOUT);
}

#[test]
fn the_login_capture_is_one_login_interactive_shell_with_the_given_limit() {
    let plan = login_capture(PathBuf::from("/fixture/bin/zsh"), SHELL_TIMEOUT);
    assert_eq!(
        plan,
        ShellCapture {
            shell: PathBuf::from("/fixture/bin/zsh"),
            args: vec![
                "-l".into(),
                "-i".into(),
                "-c".into(),
                format!("echo {START}; /usr/bin/env; echo {END}"),
            ],
            timeout: SHELL_TIMEOUT,
        }
    );
}

#[test]
fn a_shell_that_prints_its_path_is_captured_with_its_time() {
    let dir = tempfile::tempdir().unwrap();
    let shell = fake_shell(
        dir.path(),
        &format!("echo motd\necho {START}\necho PATH=/fake/login/bin:/usr/bin\necho {END}"),
    );
    let found = capture(login_capture(shell.clone(), SHELL_TIMEOUT))
        .ok()
        .unwrap();
    assert_eq!(found.path, "/fake/login/bin:/usr/bin");
    assert!(matches!(
        found.capture,
        Capture::Captured { shell: ref ran, .. } if *ran == shell
    ));
}

#[test]
fn a_shell_slower_than_the_limit_is_a_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let shell = fake_shell(
        dir.path(),
        &format!("sleep 30\necho {START}\necho PATH=/fixture/bin\necho {END}"),
    );
    let started = std::time::Instant::now();
    let error = capture(login_capture(shell.clone(), Duration::from_secs(1)))
        .err()
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        error.capture,
        Capture::Timeout {
            shell: shell.clone(),
            limit_ms: 1000
        }
    );
    assert_eq!(
        error.message,
        format!("login shell {}: no PATH within 1 seconds", shell.display())
    );
}

#[test]
fn a_shell_that_prints_no_markers_is_a_missing_path() {
    let dir = tempfile::tempdir().unwrap();
    let shell = fake_shell(dir.path(), "echo motd only");
    let error = capture(login_capture(shell.clone(), SHELL_TIMEOUT))
        .err()
        .unwrap();
    assert!(matches!(
        error.capture,
        Capture::NoPath { shell: ref ran, .. } if *ran == shell
    ));
    assert!(
        error.message.ends_with(": printed no PATH"),
        "{}",
        error.message
    );
}

#[test]
fn a_shell_that_cannot_start_is_a_start_failure_with_the_error_kind_only() {
    let dir = tempfile::tempdir().unwrap();
    let shell = dir.path().join("no-such-shell");
    let error = capture(login_capture(shell.clone(), SHELL_TIMEOUT))
        .err()
        .unwrap();
    assert_eq!(
        error.capture,
        Capture::StartFailed {
            shell,
            error: "entity not found".into()
        }
    );
}

#[test]
fn background_child_holding_stdout_does_not_block() {
    // A startup file that leaves a process holding stdout open: the
    // result arrives at the end marker, long before that process ends.
    let dir = tempfile::tempdir().unwrap();
    let shell = fake_shell(
        dir.path(),
        &format!("sleep 30 &\necho {START}\necho PATH=/fake/bin\necho {END}"),
    );
    let started = std::time::Instant::now();
    let found = shell_path(&shell, &[], Duration::from_secs(5)).unwrap().0;
    assert_eq!(found, "/fake/bin");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn silent_shell_times_out_and_its_group_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let ready = dir.path().join("shell.pid");
    let marker = dir.path().join("child.pid");
    let shell = fake_shell(
        dir.path(),
        &format!(
            "echo $$ > {}\nsleep 30 &\necho $! > {}\nwait",
            ready.display(),
            marker.display()
        ),
    );
    let started = std::time::Instant::now();
    // The shell writes its process-group leader before starting the child.
    // Two seconds keeps the fixture bounded while allowing parallel test
    // workers to schedule the tiny shell before the timeout begins.
    let error = shell_path(&shell, &[], Duration::from_secs(2)).unwrap_err();
    assert!(
        error.message.contains("no PATH within"),
        "{}",
        error.message
    );
    assert_eq!(error.failure, ShellFailure::Timeout(Duration::from_secs(2)));
    // The killed group closed the pipe, so the reader was joined.
    assert!(!error.reader_running);
    assert!(!reader_still_running());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    let shell_pid: i32 = std::fs::read_to_string(&ready)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // The background child in the shell's group was killed too.
    let pid: i32 = std::fs::read_to_string(&marker)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let pid = rustix::process::Pid::from_raw(pid).unwrap();
    let shell_pid = rustix::process::Pid::from_raw(shell_pid).unwrap();
    while rustix::process::test_kill_process(pid).is_ok() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        rustix::process::test_kill_process(pid).is_err(),
        "background child survived"
    );
    assert!(
        rustix::process::test_kill_process_group(shell_pid).is_err(),
        "shell process group survived"
    );
}

#[test]
fn absolute_environment_shell_skips_account_lookup() {
    for shell in [
        OsString::from("/environment/bin/fish"),
        OsString::from_vec(b"/environment/non-utf8-\xff".to_vec()),
    ] {
        let expected = PathBuf::from(&shell);
        let found = login_shell_with(Some(shell), 42, |_, _| {
            panic!("an absolute SHELL must not query the account database")
        });
        assert_eq!(found, expected);
    }
}

#[test]
fn missing_or_relative_environment_shell_uses_the_requested_account() {
    for shell in [
        None,
        Some(OsString::new()),
        Some(OsString::from("bin/fish")),
    ] {
        let found = login_shell_with(shell, 42, |uid, _| {
            assert_eq!(uid, 42);
            Ok(Some(OsString::from("/account/bin/fish")))
        });
        assert_eq!(found, PathBuf::from("/account/bin/fish"));
    }
}

#[test]
fn missing_invalid_or_failed_account_queries_use_the_platform_default() {
    let expected = PathBuf::from(if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    });
    for response in [
        Ok(None),
        Err(libc::EIO),
        Err(libc::ENOENT),
        Ok(Some(OsString::new())),
        Ok(Some(OsString::from("relative/shell"))),
        Ok(Some(OsString::from_vec(b"/account/non-utf8-\xff".to_vec()))),
    ] {
        let mut queries = 0;
        let found = login_shell_with(None, 42, |_, _| {
            queries += 1;
            response.clone()
        });
        assert_eq!(found, expected);
        assert_eq!(queries, 1, "only ERANGE permits a retry");
    }
}

#[test]
fn range_errors_grow_the_owned_buffer_and_preserve_the_uid() {
    let mut sizes = Vec::new();
    let found = login_shell_with(None, 42, |uid, buffer| {
        assert_eq!(uid, 42);
        sizes.push(buffer.len());
        if sizes.len() < 3 {
            return Err(libc::ERANGE);
        }
        let shell = b"/account/retry-shell";
        buffer[..shell.len()].copy_from_slice(shell);
        Ok(Some(OsString::from_vec(buffer[..shell.len()].to_vec())))
    });
    assert_eq!(found, PathBuf::from("/account/retry-shell"));
    assert_eq!(sizes.len(), 3);
    assert!(sizes.windows(2).all(|pair| pair[1] > pair[0]));
}

#[test]
fn repeated_range_errors_stop_at_one_mebibyte_and_use_the_default() {
    let mut requested_sizes = Vec::new();
    let found = login_shell_with(None, 42, |uid, buffer| {
        assert_eq!(uid, 42);
        assert!(
            buffer.len() <= 1_048_576,
            "account lookup scratch buffer exceeded 1 MiB"
        );
        requested_sizes.push(buffer.len());
        assert!(requested_sizes.len() <= 11, "account lookup did not stop");
        Err(libc::ERANGE)
    });
    assert_eq!(requested_sizes.last(), Some(&1_048_576));
    assert_eq!(
        found,
        PathBuf::from(if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/sh"
        })
    );
}

#[test]
fn reentrant_native_queries_return_owned_results_across_threads() {
    let uid = rustix::process::getuid().as_raw();
    let expected = login_shell_with(None, uid, query_account_shell);
    let ready = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    ready.wait();
                    login_shell_with(None, uid, query_account_shell)
                })
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap(), expected);
        }
    });
}
