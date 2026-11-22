//! Private mise bootstrap and bounded command execution.
use super::REQUIRED_MISE_VERSION;
use anyhow::{Context, Result, bail, ensure};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
const MISE_TIMEOUT_SECONDS: u64 = 300;
const MISE_POLL_MILLIS: u64 = 100;
pub(super) fn bundled_mise() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()?
                .parent()
                .map(|contents| contents.join("Resources/bin/mise"))
        })
        .filter(|path| path.is_file())
}
pub(super) fn verify_mise(path: &Path) -> Result<()> {
    let output = Command::new(path)
        .arg("--version")
        .env_clear()
        .output()
        .with_context(|| format!("run mise version check: {}", path.display()))?;
    ensure!(
        output.status.success(),
        "mise version check failed for {}",
        path.display()
    );
    let version = String::from_utf8_lossy(&output.stdout);
    ensure!(
        has_required_mise_version(&version),
        "mise {} is required, found {}",
        REQUIRED_MISE_VERSION,
        version.trim()
    );
    Ok(())
}
pub(super) fn has_required_mise_version(output: &str) -> bool {
    output
        .split_whitespace()
        .any(|token| token.trim_start_matches('v') == REQUIRED_MISE_VERSION)
}
pub(super) fn which(name: &str) -> Option<PathBuf> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| PathBuf::from(value.trim()))
}
pub(super) fn run_mise(mise: &Path, root: &Path, project: &Path, args: &[&str]) -> Result<String> {
    let roots = canonical_roots(root, project)?;
    let mut command = private_command(mise, &roots, args)?;
    let child = command.spawn().context("spawn private mise")?;
    wait_for_command(child)
}
struct PrivateRoots {
    root: PathBuf,
    project: PathBuf,
}
fn canonical_roots(root: &Path, project: &Path) -> Result<PrivateRoots> {
    Ok(PrivateRoots {
        root: fs::canonicalize(root).context("Resolve private mise root")?,
        project: fs::canonicalize(project).context("Resolve private project")?,
    })
}
fn private_command(mise: &Path, roots: &PrivateRoots, args: &[&str]) -> Result<Command> {
    let project = &roots.project;
    let ceiling = project.parent().context("Private project has no parent")?;
    let mut command = Command::new(mise);
    command
        .args(args)
        .current_dir(project)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("HOME", roots.root.join("private-home"))
        .env(
            "PATH",
            format!(
                "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                roots.root.join("bin").display()
            ),
        )
        .env("MISE_CONFIG_DIR", roots.root.join("mise-config"))
        .env(
            "MISE_GLOBAL_CONFIG_FILE",
            roots.root.join("mise-config/config.toml"),
        )
        .env("MISE_DATA_DIR", roots.root.join("mise-data"))
        .env("MISE_CACHE_DIR", roots.root.join("mise-cache"))
        .env("MISE_STATE_DIR", roots.root.join("mise-state"))
        .env("MISE_CEILING_PATHS", ceiling)
        .env(
            "MISE_SYSTEM_CONFIG_FILE",
            roots.root.join("mise-config/empty-system.toml"),
        )
        .env("MISE_GITHUB_GH_CLI_TOKENS", "0")
        .env("MISE_GITHUB_USE_GIT_CREDENTIALS", "0")
        .env("MISE_DISABLE_BACKENDS", "asdf,vfox")
        .env("MISE_ALL_COMPILE", "0")
        .env("TMPDIR", roots.root.join("tmp"))
        .env("MISE_TRUSTED_CONFIG_PATHS", project)
        .env("MISE_YES", "1");
    Ok(command)
}
fn wait_for_command(mut child: std::process::Child) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(MISE_TIMEOUT_SECONDS);
    loop {
        if let Some(status) = child.try_wait()? {
            let output = child.wait_with_output()?;
            if !status.success() {
                bail!("mise failed: {}", String::from_utf8_lossy(&output.stderr));
            }
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            bail!("private mise command timed out");
        }
        thread::sleep(Duration::from_millis(MISE_POLL_MILLIS));
    }
}
