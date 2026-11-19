use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use mxc_sdk::v1::{self, MxcProcess, StreamCloser, WaitResult};

use crate::SpawnRequest;

const STOP_REAP_TIMEOUT: Duration = Duration::from_secs(5);
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// The SDK stays behind the platform boundary, including its stream closers.
pub(crate) struct PersistentProcess {
    process: MxcProcess,
    stdout_closer: Option<StreamCloser>,
    stderr_closer: Option<StreamCloser>,
}

pub(crate) fn spawn(request: SpawnRequest) -> Result<PersistentProcess> {
    let args: Vec<_> = request.args.iter().map(String::as_str).collect();
    let prepared = super::prepare_request(
        &request.executable,
        &args,
        &request.readonly_roots,
        &request.workspace,
        request.proxy.as_deref(),
    )?;
    let prepared = super::policy::persistent_request(prepared, request.environment);
    super::preflight()?;
    let process = v1::spawn(prepared, Default::default())
        .map_err(|_| anyhow!("MXC persistent worker launch failed"))?;
    Ok(PersistentProcess::new(process))
}

impl PersistentProcess {
    fn new(process: MxcProcess) -> Self {
        Self {
            stdout_closer: process.stdout_closer(),
            stderr_closer: process.stderr_closer(),
            process,
        }
    }

    pub(crate) fn id(&self) -> u32 {
        self.process.id()
    }

    pub(crate) fn take_stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        self.process.take_stdin()
    }

    pub(crate) fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.process.take_stdout()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.process.take_stderr()
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<i32>> {
        self.process.try_wait()
    }

    pub(crate) fn wait(&mut self) -> io::Result<i32> {
        exit_code(self.process.wait()?)
    }

    pub(crate) fn stop(&mut self) -> io::Result<i32> {
        let deadline = Instant::now() + STOP_REAP_TIMEOUT;
        let killed = self.process.kill();
        self.close_outputs();
        // mxc-sdk 1.0.1 sends SIGKILL to the whole session, then fails with
        // EPERM when its sweep meets a member that is still exiting, often the
        // killed worker itself. Its own kill and Drop then skip the reap. A
        // later sweep sees that member gone, so retry before reaping.
        kill_until(deadline, killed, || self.process.kill())?;
        // Persistent requests have no SDK job timeout. Never use wait here:
        // a killed but unreapable child must not block the supervisor forever.
        reap_until(deadline, || self.process.try_wait())
    }

    pub(crate) fn close_outputs(&self) {
        if let Some(closer) = &self.stdout_closer {
            closer.close();
        }
        if let Some(closer) = &self.stderr_closer {
            closer.close();
        }
    }
}

impl Drop for PersistentProcess {
    fn drop(&mut self) {
        // Seatbelt's own Drop ignores a failed session kill and then leaves
        // the worker unreaped, so stop it here within the same bound.
        let _ = self.stop();
    }
}

fn exit_code(result: WaitResult) -> io::Result<i32> {
    match result {
        WaitResult::Exited(code) => Ok(code),
        WaitResult::TimedOut => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "persistent contained worker unexpectedly timed out",
        )),
    }
}

/// Retries a failed session kill until one succeeds. At the deadline the
/// last failure is the result.
fn kill_until(
    deadline: Instant,
    first: io::Result<()>,
    mut kill: impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    let mut result = first;
    while let Err(error) = result {
        let Some(remaining) = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
        else {
            return Err(error);
        };
        std::thread::sleep(remaining.min(REAP_POLL_INTERVAL));
        result = kill();
    }
    Ok(())
}

fn reap_until(
    deadline: Instant,
    mut observe: impl FnMut() -> io::Result<Option<i32>>,
) -> io::Result<i32> {
    loop {
        if let Some(code) = observe()? {
            return Ok(code);
        }
        // Reaching the deadline exactly is a timeout, not a zero-length sleep.
        let Some(remaining) = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
        else {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "contained worker did not become reapable within five seconds",
            ));
        };
        std::thread::sleep(remaining.min(REAP_POLL_INTERVAL));
    }
}

#[cfg(test)]
mod tests {
    use super::{STOP_REAP_TIMEOUT, kill_until, reap_until};
    use std::io;
    use std::time::Instant;

    /// EPERM on macOS: what the SDK's session sweep reports for an exiting member.
    const EPERM: i32 = 1;

    fn exiting_member() -> io::Error {
        io::Error::from_raw_os_error(EPERM)
    }

    #[test]
    fn a_kill_that_meets_an_exiting_member_is_retried_until_it_succeeds() {
        let mut attempts = 0;
        let deadline = Instant::now() + STOP_REAP_TIMEOUT;
        let result = kill_until(deadline, Err(exiting_member()), || {
            attempts += 1;
            if attempts < 3 {
                Err(exiting_member())
            } else {
                Ok(())
            }
        });
        assert!(result.is_ok());
        assert_eq!(attempts, 3);
    }

    #[test]
    fn a_successful_kill_is_not_repeated_and_a_lasting_failure_ends_at_the_deadline() {
        let unexpected = || Err(io::Error::other("killed again"));
        assert!(kill_until(Instant::now(), Ok(()), unexpected).is_ok());
        let error = kill_until(Instant::now(), Err(exiting_member()), unexpected).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(EPERM));
    }

    #[test]
    fn reap_deadline_reports_timeout_instead_of_polling_indefinitely() {
        let mut observed_once = false;
        let error = reap_until(Instant::now(), || {
            if observed_once {
                return Err(io::Error::other("polled after the reap deadline"));
            }
            observed_once = true;
            Ok(None)
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn reap_returns_an_already_observed_exit_even_at_the_deadline() {
        assert_eq!(reap_until(Instant::now(), || Ok(Some(23))).unwrap(), 23);
    }
}
