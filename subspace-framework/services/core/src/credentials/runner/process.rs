//! Concurrent pipe I/O with one deadline for process exit and stdout.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use super::{CapturedOutput, RunnerRequest};
use crate::path::find_tool;

const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_OUTPUT_BYTES: u64 = crate::http::MAX_BODY_BYTES + crate::http::ERROR_BODY_BYTES;

pub(super) fn capture(request: RunnerRequest<'_>) -> Result<CapturedOutput, String> {
    let deadline = Instant::now() + request.timeout;
    let mut child = spawn(&request)?;
    let result = capture_pipes(&mut child, &request, deadline);
    finish_child(&mut child, result)
}

fn finish_child(
    child: &mut Child,
    result: Result<CapturedOutput, String>,
) -> Result<CapturedOutput, String> {
    // Also reap on setup/I/O/wait errors. Never leave a child running after failure.
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn capture_pipes(
    child: &mut Child,
    request: &RunnerRequest<'_>,
    deadline: Instant,
) -> Result<CapturedOutput, String> {
    let reader = read_stdout(child)?;
    let status = wait_for_exit(child, request.program, deadline)?;
    let stdout = receive(reader, deadline)?;
    Ok(CapturedOutput::new(status.code(), stdout))
}

fn command_for(executable: PathBuf, request: &RunnerRequest<'_>) -> Command {
    let mut command = Command::new(executable);
    configure(&mut command, request);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn configure(command: &mut Command, request: &RunnerRequest<'_>) {
    command
        .args(request.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for name in request.remove {
        command.env_remove(name);
    }
}

fn spawn(request: &RunnerRequest<'_>) -> Result<Child, String> {
    let program = request.program;
    let executable = find_tool(program).map_err(|_| format!("{program} is not on the PATH"))?;
    command_for(executable, request)
        .spawn()
        .map_err(|error| format!("cannot start {program}: {error}"))
}

fn read_stdout(child: &mut Child) -> Result<Receiver<Result<String, String>>, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "CLI has no output".to_owned())?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut output = String::new();
        let result = stdout
            .take(MAX_OUTPUT_BYTES + 1)
            .read_to_string(&mut output);
        let result = match result {
            Ok(_) if output.len() as u64 <= MAX_OUTPUT_BYTES => Ok(output),
            _ => Err("cannot read CLI output within the response limit".into()),
        };
        let _ = sender.send(result);
    });
    Ok(receiver)
}

fn receive<T>(receiver: Receiver<Result<T, String>>, deadline: Instant) -> Result<T, String> {
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "CLI I/O did not finish within the timeout".to_owned())?
}

fn wait_for_exit(
    child: &mut Child,
    program: &str,
    deadline: Instant,
) -> Result<ExitStatus, String> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(WAIT_POLL_INTERVAL),
            Ok(None) => return Err(format!("{program} did not finish within the timeout")),
            Err(error) => return Err(format!("cannot wait for {program}: {error}")),
        }
    }
}
