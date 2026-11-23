//! Shared process startup for the in-crate and native-account NETT tests.

use std::io::{BufRead as _, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

pub(super) struct NettProcess {
    child: Child,
    pub(super) origin: String,
}

impl NettProcess {
    pub(super) fn start(features: &[&str], extra: &[&str]) -> Self {
        let script =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/nett_api_test_server.py");
        let mut child = python()
            .arg(&script)
            .args(["--port", "0"])
            .args(features.iter().flat_map(|feature| ["--serve", feature]))
            .args(extra)
            .spawn()
            .expect("python3 runs the NETT mock");
        // Python loads the whole mock before it prints its banner, which
        // takes far longer than usual on a machine busy with other builds.
        // The limit only bounds a mock that never starts.
        let origin =
            await_origin(&mut child, Duration::from_secs(60)).expect("the NETT mock started");
        Self { child, origin }
    }
}

pub(super) fn python() -> Command {
    let mut command = Command::new(
        subspace_framework::core::path::find_tool("python3").expect("python3 runs the NETT mock"),
    );
    command
        .arg("-I")
        .env_clear()
        // Windows Python needs SYSTEMROOT to open a socket.
        .envs(std::env::var_os("SYSTEMROOT").map(|root| ("SYSTEMROOT", root)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

pub(super) fn await_origin(child: &mut Child, timeout: Duration) -> Result<String, &'static str> {
    let origin = read_banner(child, timeout);
    if origin.is_err() {
        stop(child);
    }
    origin
}

fn read_banner(child: &mut Child, timeout: Duration) -> Result<String, &'static str> {
    let stdout = child.stdout.take().ok_or("missing fixture stdout")?;
    let (send, receive) = mpsc::channel();
    std::thread::Builder::new()
        .spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = send.send(result);
        })
        .map_err(|_| "could not start the fixture banner reader")?;
    let line = receive
        .recv_timeout(timeout)
        .map_err(|_| "fixture startup timed out")?
        .map_err(|_| "could not read the fixture banner")?;
    line.trim()
        .strip_prefix("NETT fixture listening on ")
        .map(str::to_owned)
        .ok_or("unexpected fixture banner")
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

impl Drop for NettProcess {
    fn drop(&mut self) {
        stop(&mut self.child);
    }
}
