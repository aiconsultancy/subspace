use std::fmt;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use anyhow::Result;

use crate::{backend, selected_environment};

/// Explicit launch inputs for a persistent contained worker.
///
/// Only selected environment values reach the worker, alongside managed HOME,
/// TMPDIR, PATH and proxy settings. The child can read and transmit raw delegated
/// values. Put credentials in `environment`, never in arguments or source.
/// Environment values, arguments and the proxy are omitted from Debug output.
#[non_exhaustive]
pub struct SpawnRequest {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub readonly_roots: Vec<PathBuf>,
    pub workspace: PathBuf,
    pub proxy: Option<String>,
    pub environment: Vec<(String, String)>,
}

impl SpawnRequest {
    pub fn new(executable: PathBuf, readonly_roots: Vec<PathBuf>, workspace: PathBuf) -> Self {
        Self {
            executable,
            args: Vec::new(),
            readonly_roots,
            workspace,
            proxy: None,
            environment: Vec::new(),
        }
    }
}

impl fmt::Debug for SpawnRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SpawnRequest")
            .field("has_executable", &!self.executable.as_os_str().is_empty())
            .field("readonly_root_count", &self.readonly_roots.len())
            .field("has_workspace", &!self.workspace.as_os_str().is_empty())
            .field("argument_count", &self.args.len())
            .field("has_proxy", &self.proxy.is_some())
            .field("selected_environment_count", &self.environment.len())
            .finish_non_exhaustive()
    }
}

/// Spawn with no job timeout. The owner must stop or drop the returned handle.
/// Unsupported platforms fail closed without launching a process.
pub fn spawn(request: SpawnRequest) -> Result<PersistentProcess> {
    selected_environment::validate_selected_environment(&request.environment)?;
    backend::spawn(request).map(|inner| PersistentProcess { inner })
}

/// Owns a contained process independently of its transferred pipes.
///
/// Stop kills ordinary contained descendants, reaps the worker and cancels output
/// reads. Drop uses the SDK's bounded kill/reap cleanup. This does not promise
/// cleanup after a host crash or for deliberately detached descendants.
/// Blocking I/O and lifecycle methods belong off the UI thread. The supervisor
/// owns pipe reader/writer tasks, framing limits and joining those tasks.
pub struct PersistentProcess {
    inner: backend::PersistentProcess,
}

impl PersistentProcess {
    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    /// Transfer stdin once. Drop the writer to send EOF.
    pub fn take_stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        self.inner.take_stdin()
    }

    /// Transfer stdout once. Drain both output pipes concurrently.
    pub fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.inner.take_stdout()
    }

    pub fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.inner.take_stderr()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<i32>> {
        self.inner.try_wait()
    }

    /// Wait for exit. Taken output pipes must be drained by their owners.
    pub fn wait(&mut self) -> io::Result<i32> {
        self.inner.wait()
    }

    /// Stop execution and reap the worker, even when stdin is held elsewhere.
    pub fn stop(&mut self) -> io::Result<i32> {
        self.inner.stop()
    }

    /// Cancel blocked output reads without stopping execution. Remaining output is lost.
    pub fn close_outputs(&self) {
        self.inner.close_outputs();
    }
}

impl fmt::Debug for PersistentProcess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentProcess")
            .finish_non_exhaustive()
    }
}
