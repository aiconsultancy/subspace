use super::{SessionSnapshot, SessionStatus, framing::read_frame, state::RuntimeState};
use crate::project::{HostMessage, WorkerMessage};
use anyhow::{Context, Result};
use std::io::{BufReader, Read, Write};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::proxy::PersistentProxy;
use crate::environments::persistent::PreparedRuntime;

const POLL: Duration = Duration::from_millis(20);
const STDERR_READ_BYTES: usize = 4096;

pub(super) struct Owner {
    pub(super) state: Arc<Mutex<RuntimeState>>,
    commands: mpsc::SyncSender<Vec<u8>>,
    cancelled: Arc<AtomicBool>,
    supervisor: Mutex<Option<JoinHandle<()>>>,
}

pub(super) fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    value
        .lock()
        .map_err(|_| anyhow::anyhow!("session state lock failed"))
}

impl Owner {
    pub(super) fn start(
        mut prepared: PreparedRuntime,
        mut state: RuntimeState,
        proxy: PersistentProxy,
    ) -> Result<Self> {
        prepared.request.proxy = Some(proxy.address());
        let init = state.init()?.to_line()?;
        let mut process = mxc_probe::spawn(prepared.request)?;
        let stdin = process.take_stdin().context("worker stdin unavailable")?;
        let stdout = process.take_stdout().context("worker stdout unavailable")?;
        let stderr = process.take_stderr().context("worker stderr unavailable")?;
        let state = Arc::new(Mutex::new(state));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (commands, receiver) = mpsc::sync_channel(super::MAX_PENDING);
        commands
            .try_send(init)
            .context("could not queue host initialization")?;
        let pipes = PipeOwner {
            state: state.clone(),
            cancelled: cancelled.clone(),
        };
        let readers = pipes.start(stdin, stdout, stderr, receiver);
        let thread_state = state.clone();
        let thread_cancel = cancelled.clone();
        let supervisor = thread::spawn(move || {
            supervise(&mut process, &thread_state, &thread_cancel);
            drop(proxy);
            for reader in readers {
                let _ = reader.join();
            }
            drop(prepared.source);
            drop(prepared.workspace);
        });
        Ok(Self {
            state,
            commands,
            cancelled,
            supervisor: Mutex::new(Some(supervisor)),
        })
    }

    pub(super) fn queue(&self, message: HostMessage) -> Result<()> {
        if self.commands.try_send(message.to_line()?).is_err() {
            lock(&self.state)?.fail("worker command queue is unavailable or full");
            self.cancelled.store(true, Ordering::Release);
            anyhow::bail!("worker command queue is unavailable or full");
        }
        Ok(())
    }

    pub(super) fn snapshot(&self) -> Result<SessionSnapshot> {
        let state = lock(&self.state)?;
        Ok(SessionSnapshot {
            live_state: state.live().clone(),
            status: state.status(),
            diagnostics: state.diagnostics().into(),
            pending: state.pending(),
        })
    }

    pub(super) fn stop(&self) -> Result<()> {
        lock(&self.state)?.stop();
        self.cancelled.store(true, Ordering::Release);
        if let Some(supervisor) = lock(&self.supervisor)?.take() {
            supervisor
                .join()
                .map_err(|_| anyhow::anyhow!("worker supervisor panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn supervise(
    process: &mut mxc_probe::PersistentProcess,
    state: &Mutex<RuntimeState>,
    cancelled: &AtomicBool,
) {
    while !cancelled.load(Ordering::Acquire) {
        match process.try_wait() {
            Ok(None) => thread::sleep(POLL),
            Ok(Some(_)) => {
                fail(
                    state,
                    cancelled,
                    "worker exited; explicit Restart is required",
                );
                break;
            }
            Err(_) => {
                fail(state, cancelled, "worker process status failed");
                break;
            }
        }
    }
    if process.stop().is_err()
        && let Ok(mut state) = state.lock()
    {
        state.fail("contained worker cleanup failed");
    }
    process.close_outputs();
}

fn fail(state: &Mutex<RuntimeState>, cancelled: &AtomicBool, message: &str) {
    if !cancelled.swap(true, Ordering::AcqRel)
        && let Ok(mut state) = state.lock()
    {
        state.fail(message);
    }
}

#[cfg(test)]
mod tests {
    use super::{Owner, lock};
    use crate::project::InputValue;
    use crate::project_runtime::tests::running;
    use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc};

    #[test]
    fn snapshot_reads_new_accepted_state_without_a_cache_delay() {
        let (commands, _receiver) = mpsc::sync_channel(1);
        let owner = Owner {
            state: Arc::new(Mutex::new(running())),
            commands,
            cancelled: Arc::new(AtomicBool::new(false)),
            supervisor: Mutex::new(None),
        };
        assert_eq!(owner.snapshot().unwrap().live_state().input_revision(), 0);
        lock(&owner.state)
            .unwrap()
            .set_value("amount", InputValue::Number(7.0), 0)
            .unwrap();
        assert_eq!(owner.snapshot().unwrap().live_state().input_revision(), 1);
    }
}

#[derive(Clone)]
struct PipeOwner {
    state: Arc<Mutex<RuntimeState>>,
    cancelled: Arc<AtomicBool>,
}

impl PipeOwner {
    fn start(
        self,
        stdin: Box<dyn Write + Send>,
        stdout: Box<dyn Read + Send>,
        stderr: Box<dyn Read + Send>,
        receiver: mpsc::Receiver<Vec<u8>>,
    ) -> Vec<JoinHandle<()>> {
        let writer = self.clone();
        let errors = self.clone();
        vec![
            thread::spawn(move || writer.write(stdin, receiver)),
            thread::spawn(move || self.read(stdout)),
            thread::spawn(move || errors.stderr(stderr)),
        ]
    }

    fn write(&self, mut stdin: Box<dyn Write + Send>, receiver: mpsc::Receiver<Vec<u8>>) {
        while !self.cancelled.load(Ordering::Acquire) {
            match receiver.recv_timeout(POLL) {
                Ok(bytes) => {
                    if stdin
                        .write_all(&bytes)
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        fail(&self.state, &self.cancelled, "worker input pipe failed");
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn read(&self, stdout: Box<dyn Read + Send>) {
        let mut reader = BufReader::new(stdout);
        while !self.cancelled.load(Ordering::Acquire) {
            let message = match read_frame(&mut reader)
                .and_then(|frame| frame.context("worker protocol pipe closed"))
                .and_then(|frame| WorkerMessage::parse(&frame))
            {
                Ok(message) => message,
                Err(_) => {
                    fail(
                        &self.state,
                        &self.cancelled,
                        "malformed, oversized or closed worker protocol stream",
                    );
                    break;
                }
            };
            if let Ok(mut state) = self.state.lock() {
                if self.cancelled.load(Ordering::Acquire) {
                    break;
                }
                if let Err(error) = state.accept(message) {
                    state.stderr(format!("{error}\n").as_bytes());
                }
                if state.status() == SessionStatus::Failed {
                    self.cancelled.store(true, Ordering::Release);
                }
            } else {
                self.cancelled.store(true, Ordering::Release);
            }
        }
    }

    fn stderr(&self, mut stderr: Box<dyn Read + Send>) {
        let mut buffer = [0u8; STDERR_READ_BYTES];
        while !self.cancelled.load(Ordering::Acquire) {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if let Ok(mut state) = self.state.lock() {
                        state.stderr(&buffer[..count]);
                    }
                }
            }
        }
    }
}
