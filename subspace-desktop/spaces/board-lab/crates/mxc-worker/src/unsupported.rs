use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::SpawnRequest;
use crate::platform::unsupported_message;

pub(crate) struct PersistentProcess {
    unavailable: std::convert::Infallible,
}

pub(crate) fn spawn(_request: SpawnRequest) -> Result<PersistentProcess> {
    unsupported()
}

impl PersistentProcess {
    pub(crate) fn id(&self) -> u32 {
        match self.unavailable {}
    }

    pub(crate) fn take_stdin(&self) -> Option<Box<dyn std::io::Write + Send>> {
        match self.unavailable {}
    }

    pub(crate) fn take_stdout(&self) -> Option<Box<dyn std::io::Read + Send>> {
        match self.unavailable {}
    }

    pub(crate) fn take_stderr(&self) -> Option<Box<dyn std::io::Read + Send>> {
        match self.unavailable {}
    }

    pub(crate) fn try_wait(&self) -> std::io::Result<Option<i32>> {
        match self.unavailable {}
    }

    pub(crate) fn wait(&self) -> std::io::Result<i32> {
        match self.unavailable {}
    }

    pub(crate) fn stop(&self) -> std::io::Result<i32> {
        match self.unavailable {}
    }

    pub(crate) fn close_outputs(&self) {
        match self.unavailable {}
    }
}

pub(crate) fn capabilities() -> String {
    "unsupported".to_string()
}

pub(crate) fn run(
    _executable: &Path,
    _args: &[&str],
    _readonly_root: &Path,
    _workspace: &Path,
) -> Result<String> {
    unsupported()
}

pub(crate) fn run_scoped(
    _executable: &Path,
    _args: &[&str],
    _readonly_roots: &[PathBuf],
    _workspace: &Path,
    _proxy: Option<&str>,
) -> Result<String> {
    unsupported()
}

pub(crate) fn probe(
    _executable: &Path,
    _readonly_root: &Path,
    _workspace: &Path,
) -> Result<String> {
    unsupported()
}

fn unsupported<T>() -> Result<T> {
    bail!("{}", unsupported_message())
}
