use std::fs;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::paths;
use crate::probe_script::probe_script;

const CANARY_VALUE: &str = "mxc-probe-canary-v1";
const WORKSPACE_FILE: &str = "mxc-probe-workspace.txt";
const WORKSPACE_VALUE: &str = "workspace-write-ok";

pub(crate) fn run(executable: &Path, readonly_root: &Path, workspace: &Path) -> Result<String> {
    let readonly_root = paths::resolve_directory(readonly_root, "readonly runtime root")?;
    let workspace = paths::resolve_workspace(workspace)?;
    paths::reject_overlap(&readonly_root, &workspace)?;
    run_probe(executable, &readonly_root, &workspace)
}

fn run_probe(executable: &Path, readonly_root: &Path, workspace: &Path) -> Result<String> {
    let canary_dir = unique_canary_dir(paths::canary_parent(workspace))?;
    settle_probe(
        probe_with_canary(executable, readonly_root, workspace, &canary_dir),
        remove_canary(&canary_dir),
    )
}

fn settle_probe(result: Result<String>, cleanup: Result<()>) -> Result<String> {
    let output = result?;
    cleanup?;
    Ok(output)
}

fn remove_canary(canary_dir: &Path) -> Result<()> {
    fs::remove_dir_all(canary_dir).context("remove synthetic canary")
}

fn probe_with_canary(
    executable: &Path,
    readonly_root: &Path,
    workspace: &Path,
    canary_dir: &Path,
) -> Result<String> {
    let canary = write_canary(canary_dir, CANARY_VALUE)?;
    verify_unsandboxed_canary(&canary, CANARY_VALUE)?;
    let (listener, address) = bind_loopback()?;
    confirm_loopback(address)?;
    let script = probe_script(&canary, workspace, address.port());
    let output = super::run(executable, &["-c", &script], readonly_root, workspace)?;
    verify_probe_files(&canary, CANARY_VALUE, workspace)?;
    drop(listener);
    Ok(output)
}

fn write_canary(canary_dir: &Path, expected: &str) -> Result<PathBuf> {
    let canary = canary_dir.join("canary.txt");
    fs::write(&canary, expected).context("create synthetic canary")?;
    Ok(canary)
}

fn verify_unsandboxed_canary(canary: &Path, expected: &str) -> Result<()> {
    if fs::read_to_string(canary).context("unsandboxed canary read")? != expected {
        bail!("unsandboxed canary baseline failed");
    }
    Ok(())
}

fn bind_loopback() -> Result<(TcpListener, SocketAddr)> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind loopback canary")?;
    let address = listener
        .local_addr()
        .context("read loopback canary address")?;
    Ok((listener, address))
}

fn confirm_loopback(address: SocketAddr) -> Result<()> {
    TcpStream::connect(address).context("unsandboxed loopback canary connect")?;
    Ok(())
}

fn unique_canary_dir(parent: &Path) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = parent.join(format!(".mxc-probe-canary-{}-{stamp}", std::process::id()));
    fs::create_dir(&path).context("create unique synthetic canary directory")?;
    Ok(path)
}

fn verify_probe_files(canary: &Path, expected: &str, workspace: &Path) -> Result<()> {
    let contents = fs::read_to_string(canary).context("verify canary contents")?;
    if contents != expected {
        bail!("canary contents changed");
    }
    let workspace_file = workspace.join(WORKSPACE_FILE);
    let workspace_contents = fs::read_to_string(workspace_file).context("verify workspace file")?;
    if workspace_contents != WORKSPACE_VALUE {
        bail!("workspace write postcondition failed");
    }
    Ok(())
}
