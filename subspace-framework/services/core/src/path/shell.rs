//! Login-shell `PATH` capture for Unix.

use std::ffi::OsString;
use std::io::IsTerminal as _;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::Capture;
use super::output::{parse_network_variables, parse_shell_output};
use super::resolve::{CaptureError, Found, Source};
use super::{END, START};

/// Whether this process counts as started from a terminal: stdin is one,
/// and `force` does not ask for the login shell anyway.
pub(super) fn terminal_start(force: bool) -> bool {
    !force && std::io::stdin().is_terminal()
}

/// Ask the account's login shell for its `PATH`, waiting at most `timeout`.
pub(super) fn platform_path(timeout: Duration) -> Result<Found, CaptureError> {
    capture(login_capture(login_shell(), timeout))
}

/// Why [`shell_path`] got no `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ShellFailure {
    /// No complete result within the limit.
    Timeout(Duration),
    /// No markers, or no non-empty `PATH` between them.
    NoPath,
    /// The shell could not start; the io::ErrorKind text.
    StartFailed(String),
    Other,
}

/// A failed [`shell_path`]: the class and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ShellError {
    pub(super) failure: ShellFailure,
    pub(super) message: String,
    /// After a timeout: the output reader could not be joined, because a
    /// process that left the shell's group still holds the pipe.
    pub(super) reader_running: bool,
}

impl ShellError {
    fn new(failure: ShellFailure, message: impl Into<String>) -> Self {
        Self {
            failure,
            message: message.into(),
            reader_running: false,
        }
    }
}

/// How long the reader may take to see end of file after the shell's
/// process group was killed.
const READER_GRACE: Duration = Duration::from_secs(1);

static READER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Whether a capture in this process left its reader thread running. Then
/// [`super::apply`] must not change the environment.
pub(super) fn reader_still_running() -> bool {
    READER_RUNNING.load(Ordering::SeqCst)
}

/// One login-shell capture: the shell, its command and its time limit.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ShellCapture {
    pub(super) shell: PathBuf,
    pub(super) args: Vec<String>,
    pub(super) timeout: Duration,
}

/// The capture [`platform_path`] runs.
pub(super) fn login_capture(shell: PathBuf, timeout: Duration) -> ShellCapture {
    // -l reads login files (~/.zprofile, where Homebrew sets its PATH) and -i
    // reads interactive ones (~/.zshrc), as a new terminal window does. The
    // markers skip anything those files print. /usr/bin/env works in zsh,
    // bash, and fish alike.
    let script = format!("echo {START}; /usr/bin/env; echo {END}");
    ShellCapture {
        shell,
        args: vec!["-l".into(), "-i".into(), "-c".into(), script],
        timeout,
    }
}

fn milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Run `plan` and describe the result as a [`Capture`].
pub(super) fn capture(plan: ShellCapture) -> Result<Found, CaptureError> {
    let args = plan.args.iter().map(String::as_str).collect::<Vec<_>>();
    let started = Instant::now();
    match shell_path(&plan.shell, &args, plan.timeout) {
        Ok((path, network)) => Ok(Found {
            path: OsString::from(path),
            source: Source::LoginShell(plan.shell.clone()),
            capture: Capture::Captured {
                shell: plan.shell,
                elapsed_ms: milliseconds(started.elapsed()),
            },
            network,
        }),
        Err(error) => Err(CaptureError {
            message: format!("login shell {}: {}", plan.shell.display(), error.message),
            capture: failed_capture(plan.shell, error, started.elapsed()),
        }),
    }
}

fn failed_capture(shell: PathBuf, error: ShellError, elapsed: Duration) -> Capture {
    match error.failure {
        ShellFailure::Timeout(limit) => Capture::Timeout {
            shell,
            limit_ms: milliseconds(limit),
        },
        ShellFailure::NoPath => Capture::NoPath {
            shell,
            elapsed_ms: milliseconds(elapsed),
        },
        ShellFailure::StartFailed(error) => Capture::StartFailed { shell, error },
        ShellFailure::Other => Capture::Other {
            reason: format!("{}: {}", shell.display(), error.message),
        },
    }
}

fn path_from_capture(captured: String) -> Result<String, ShellError> {
    parse_shell_output(&captured)
        .ok_or_else(|| ShellError::new(ShellFailure::NoPath, "printed no PATH"))
}

/// Run `shell args` and return the `PATH` it prints between the markers, with
/// the network variables it prints there.
///
/// The deadline covers reading as well as running. A program that a startup
/// file starts in the background can inherit stdout and hold it open after
/// the shell exits, so the reader returns as soon as it sees the end marker
/// instead of waiting for end of file. The shell runs in its own process
/// group; when the deadline passes, the whole group is killed, which closes
/// any pipe a descendant holds. The reader thread has ended by the time this
/// returns, unless an error reports `reader_running`; only then may another
/// thread still run.
pub(super) fn shell_path(
    shell: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<(String, Vec<(String, String)>), ShellError> {
    let captured = run_shell_capture(shell, args, timeout)?;
    let network = parse_network_variables(&captured);
    Ok((path_from_capture(captured)?, network))
}

struct ShellSession {
    child: std::process::Child,
    receiver: Receiver<String>,
    reader: JoinHandle<()>,
}

fn start_shell(shell: &Path, args: &[&str]) -> Result<ShellSession, ShellError> {
    use std::io::{BufRead, BufReader};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut child = Command::new(shell)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|error| {
            ShellError::new(
                ShellFailure::StartFailed(error.kind().to_string()),
                format!("start failed: {}", error.kind()),
            )
        })?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(ShellError::new(
            ShellFailure::Other,
            "stdout pipe unavailable",
        ));
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut captured = String::new();
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let done = line.contains(END);
            captured.push_str(&line);
            captured.push('\n');
            if done {
                break;
            }
        }
        let _ = sender.send(captured);
    });
    Ok(ShellSession {
        child,
        receiver,
        reader,
    })
}

fn finish_shell(mut session: ShellSession, timeout: Duration) -> Result<String, ShellError> {
    let outcome = session.receiver.recv_timeout(timeout);
    let group = rustix::process::Pid::from_raw(session.child.id() as i32);
    match outcome {
        Ok(captured) => {
            let _ = session.reader.join();
            let _ = session.child.kill();
            let _ = session.child.wait();
            Ok(captured)
        }
        Err(_) => {
            if let Some(group) = group {
                let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
            }
            let _ = session.child.wait();
            let reader_running = stop_reader(session.receiver, session.reader);
            Err(timeout_error(timeout, reader_running))
        }
    }
}

/// Wait at most [`READER_GRACE`] for the reader to see end of file, then
/// join it. Returns whether it still runs; that is also recorded for
/// [`reader_still_running`].
fn stop_reader(receiver: Receiver<String>, reader: JoinHandle<()>) -> bool {
    match receiver.recv_timeout(READER_GRACE) {
        Ok(_) | Err(RecvTimeoutError::Disconnected) => {
            let _ = reader.join();
            false
        }
        Err(RecvTimeoutError::Timeout) => {
            READER_RUNNING.store(true, Ordering::SeqCst);
            true
        }
    }
}

fn timeout_error(timeout: Duration, reader_running: bool) -> ShellError {
    let note = if reader_running {
        "; a process outside the shell's group still holds its output, so the PATH was not changed"
    } else {
        ""
    };
    ShellError {
        failure: ShellFailure::Timeout(timeout),
        message: format!("no PATH within {} seconds{note}", timeout.as_secs()),
        reader_running,
    }
}

fn run_shell_capture(shell: &Path, args: &[&str], timeout: Duration) -> Result<String, ShellError> {
    finish_shell(start_shell(shell, args)?, timeout)
}

const ACCOUNT_BUFFER_START_SIZE: usize = 1024;
// Limit account database scratch storage to 1 MiB before using the platform fallback.
const ACCOUNT_BUFFER_MAX_SIZE: usize = 1024 * 1024;

/// `$SHELL` when it is an absolute path, else the account's shell from the
/// password database, else the platform default.
fn login_shell() -> PathBuf {
    login_shell_with(
        std::env::var_os("SHELL"),
        rustix::process::getuid().as_raw(),
        query_account_shell,
    )
}

/// Own the account-query buffer and retry only when the native API needs more space.
fn login_shell_with(
    environment: Option<OsString>,
    uid: libc::uid_t,
    mut query: impl FnMut(libc::uid_t, &mut [u8]) -> Result<Option<OsString>, libc::c_int>,
) -> PathBuf {
    if let Some(shell) = environment.map(PathBuf::from)
        && shell.is_absolute()
    {
        return shell;
    }
    let mut buffer = vec![0; ACCOUNT_BUFFER_START_SIZE];
    loop {
        match query(uid, &mut buffer) {
            Ok(Some(shell)) if shell.to_str().is_some() && Path::new(&shell).is_absolute() => {
                return PathBuf::from(shell);
            }
            Err(libc::ERANGE) => {
                let Some(length) = buffer.len().checked_mul(2) else {
                    break;
                };
                if length > ACCOUNT_BUFFER_MAX_SIZE {
                    break;
                }
                if buffer.try_reserve_exact(length - buffer.len()).is_err() {
                    break;
                }
                buffer.resize(length, 0);
            }
            Ok(_) | Err(_) => break,
        }
    }
    PathBuf::from(if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    })
}

// qual:allow(complexity, unsafe) reason: "getpwuid_r writes to the caller-owned passwd and buffer; success and null pointers are checked before copying pw_shell while the buffer is alive. No native pointer escapes this query."
fn query_account_shell(
    uid: libc::uid_t,
    buffer: &mut [u8],
) -> Result<Option<OsString>, libc::c_int> {
    // SAFETY: zero is valid for passwd's integer and pointer fields. The record,
    // result pointer and buffer are writable, exclusive storage for this call.
    // On success libc points pw_shell to a NUL-terminated string in buffer. It
    // is null-checked and copied before the buffer can be resized or dropped.
    unsafe {
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut result = std::ptr::null_mut();
        let code = libc::getpwuid_r(
            uid,
            &mut entry,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        );
        if code != 0 {
            return Err(code);
        }
        if result.is_null() || entry.pw_shell.is_null() {
            return Ok(None);
        }
        let shell = std::ffi::CStr::from_ptr(entry.pw_shell).to_bytes();
        Ok(Some(OsString::from_vec(shell.to_vec())))
    }
}

#[cfg(test)]
mod tests;
