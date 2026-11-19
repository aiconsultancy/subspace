//! Pi executable discovery, startup validation and its process command.
use super::{
    AsyncRead, AsyncReadExt, Duration, PI_VERSION, Path, PathBuf, ProcessCommand, RPC_TIMEOUT,
    STDERR_CHUNK, SpawnConfig, Stdio, VERSION_OUTPUT_LIMIT,
};
/// Pi through the shared lookup, which also checks the Homebrew and
/// `/usr/local/bin` folders ([`subspace_core::path::FALLBACK_DIRECTORIES`]).
pub(super) fn find_pi() -> Result<PathBuf, String> {
    subspace_core::path::find_tool("pi")
        .map_err(|_| "Pi was not found. Install Pi 1.0.4, then restart Subspace.".into())
}

pub(super) async fn verify_pi(executable: &Path) -> Result<(), String> {
    verify_pi_with_timeout(executable, RPC_TIMEOUT).await
}

pub(super) async fn verify_pi_with_timeout(
    executable: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let mut command = pi_process(executable);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| "Pi could not start. Check that its Node runtime is available.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Pi did not open version output.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Pi did not open diagnostic output.".to_string())?;
    let output = tokio::spawn(read_limited(stdout, VERSION_OUTPUT_LIMIT));
    let errors = tokio::spawn(read_limited(stderr, VERSION_OUTPUT_LIMIT));
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result.map_err(|_| "Pi version detection failed.".to_string())?,
        Err(_) => {
            let _ = child.kill().await;
            return Err("Pi version detection timed out.".into());
        }
    };
    let stdout = output
        .await
        .map_err(|_| "Cannot read Pi version output.".to_string())??;
    let stderr = errors
        .await
        .map_err(|_| "Cannot read Pi diagnostic output.".to_string())??;
    let version = String::from_utf8_lossy(&stdout).trim().to_owned();
    if !status.success() || version != PI_VERSION {
        let detail = String::from_utf8_lossy(&stderr);
        return Err(format!(
            "Subspace requires Pi {PI_VERSION}; the selected executable reported {version:?}. {}",
            detail.trim()
        ));
    }
    Ok(())
}

async fn read_limited(mut reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; STDERR_CHUNK];
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|_| "Cannot read Pi process output.".to_string())?;
        if count == 0 {
            return Ok(output);
        }
        if output.len() + count > limit {
            return Err("Pi process output exceeded its limit.".into());
        }
        output.extend_from_slice(&chunk[..count]);
    }
}

pub(super) fn pi_command(config: &SpawnConfig<'_>) -> ProcessCommand {
    let mut command = pi_process(config.executable);
    command
        .args(["--mode", "rpc", "--no-extensions", "--extension"])
        .arg(config.bridge)
        .args([
            "--no-mcp",
            "--no-builtin-tools",
            "--no-context-files",
            "--no-prompt-templates",
            "--no-skills",
            "--no-themes",
            "--no-approve",
            "--session-dir",
        ])
        .arg(config.sessions);
    if let Some(saved) = config.saved {
        command.args(["--session", saved]);
    }
    if let Some(title) = config.title {
        command.args(["--name", title]);
    }
    command
        .current_dir(config.workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

pub(super) fn pi_process(executable: &Path) -> ProcessCommand {
    crate::runtime::child::agent_process(executable)
}
