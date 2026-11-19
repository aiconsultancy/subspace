//! Local agent child processes, Pi and ACP agents alike: their environment,
//! and the bounded newline-delimited JSON they write on standard output.
use super::{ChatId, lock, mpsc};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command as ProcessCommand;

/// The largest record a child may write on one line.
pub(super) const RECORD_LIMIT: usize = 1024 * 1024;
const STDERR_LIMIT: usize = 16 * 1024;
const STDERR_CHUNK: usize = 2 * 1024;
const RECORD_CHUNK: usize = 8 * 1024;

/// What a child's output reader reports to its engine.
pub(super) trait Output: Send + 'static {
    fn record(record: Value) -> Self;
    fn exited(message: String) -> Self;
}

/// A child without Subspace's service tokens, with its own folder first in
/// `PATH`, so a Node launcher finds the runtime installed beside it.
pub(super) fn agent_process(executable: &Path) -> ProcessCommand {
    let mut command = ProcessCommand::new(executable);
    command.kill_on_drop(true);
    filter_service_environment(&mut command);
    prepend_executable_path(&mut command, executable);
    command
}

fn filter_service_environment(command: &mut ProcessCommand) {
    for name in ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
        command.env_remove(name);
    }
}

fn prepend_executable_path(command: &mut ProcessCommand, executable: &Path) {
    let Some(parent) = executable.parent() else {
        return;
    };
    let mut paths = vec![parent.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
}

/// Forwards each JSON line of `stdout` to `sender`. When the stream ends, the
/// engine hears that `engine` exited, with the tail of its diagnostics.
pub(super) fn forward_output<M: Output>(
    chat: ChatId,
    engine: &'static str,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    sender: mpsc::Sender<(ChatId, M)>,
) {
    let diagnostics = Arc::new(Mutex::new(String::new()));
    let errors = diagnostics.clone();
    tokio::spawn(async move {
        read_diagnostics(stderr, errors).await;
    });
    tokio::spawn(async move {
        if !read_records(stdout, chat, engine, &sender).await {
            return;
        }
        let detail = lock(&diagnostics).trim().to_owned();
        let _ = sender
            .send((chat, M::exited(exit_message(engine, &detail))))
            .await;
    });
}

fn exit_message(engine: &str, detail: &str) -> String {
    if detail.is_empty() {
        format!("{engine} exited unexpectedly. Restart Subspace to reconnect.")
    } else {
        format!("{engine} exited unexpectedly: {detail}")
    }
}

async fn read_diagnostics(mut stderr: impl AsyncRead + Unpin, diagnostics: Arc<Mutex<String>>) {
    let mut buffer = [0_u8; STDERR_CHUNK];
    while let Ok(count) = stderr.read(&mut buffer).await {
        if count == 0 {
            return;
        }
        let mut text = lock(&diagnostics);
        let room = STDERR_LIMIT.saturating_sub(text.len());
        text.push_str(&String::from_utf8_lossy(&buffer[..count.min(room)]));
    }
}

/// Whether the stream ended by itself; `false` after a protocol error, which
/// has already been reported.
async fn read_records<M: Output>(
    mut stdout: impl AsyncRead + Unpin,
    chat: ChatId,
    engine: &'static str,
    sender: &mpsc::Sender<(ChatId, M)>,
) -> bool {
    let mut chunk = [0_u8; RECORD_CHUNK];
    let mut pending = Vec::new();
    loop {
        let Ok(count) = stdout.read(&mut chunk).await else {
            return true;
        };
        if count == 0 {
            return true;
        }
        pending.extend_from_slice(&chunk[..count]);
        if pending.len() > RECORD_LIMIT {
            let message = format!("{engine} emitted an RPC record larger than 1 MiB.");
            return send_protocol_error(chat, sender, message).await;
        }
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            if !send_record(chat, engine, sender, &line[..line.len() - 1]).await {
                return false;
            }
        }
    }
}

async fn send_record<M: Output>(
    chat: ChatId,
    engine: &'static str,
    sender: &mpsc::Sender<(ChatId, M)>,
    line: &[u8],
) -> bool {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.iter().all(u8::is_ascii_whitespace) {
        return true;
    }
    match serde_json::from_slice(line) {
        Ok(record) => sender.send((chat, M::record(record))).await.is_ok(),
        Err(_) => {
            let message = format!("{engine} emitted invalid JSON RPC output.");
            send_protocol_error(chat, sender, message).await
        }
    }
}

async fn send_protocol_error<M: Output>(
    chat: ChatId,
    sender: &mpsc::Sender<(ChatId, M)>,
    message: String,
) -> bool {
    let _ = sender.send((chat, M::exited(message))).await;
    false
}
