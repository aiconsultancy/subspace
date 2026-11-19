mod persistent;
mod policy;
mod probe;

pub(crate) use persistent::{PersistentProcess, spawn};

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use mxc_sdk::v1::{self, ContainerRequest, ExecutionResult, WaitResult};

use crate::env;
use crate::output::format_output;
use crate::paths;
use crate::quote;

pub(crate) fn capabilities() -> String {
    format!("{:?}", v1::platform_support())
}

pub(crate) fn run(
    executable: &Path,
    args: &[&str],
    readonly_root: &Path,
    workspace: &Path,
) -> Result<String> {
    run_scoped(
        executable,
        args,
        &single_root(readonly_root),
        workspace,
        None,
    )
}

fn single_root(readonly_root: &Path) -> [PathBuf; 1] {
    [readonly_root.to_owned()]
}

pub(crate) fn run_scoped(
    executable: &Path,
    args: &[&str],
    readonly_roots: &[PathBuf],
    workspace: &Path,
    proxy: Option<&str>,
) -> Result<String> {
    let request = prepare_request(executable, args, readonly_roots, workspace, proxy)?;
    execute(request)
}

pub(crate) fn probe(executable: &Path, readonly_root: &Path, workspace: &Path) -> Result<String> {
    probe::run(executable, readonly_root, workspace)
}

fn prepare_request(
    executable: &Path,
    args: &[&str],
    readonly_roots: &[PathBuf],
    workspace: &Path,
    proxy: Option<&str>,
) -> Result<ContainerRequest> {
    let executable = resolve_executable(executable)?;
    let workspace = paths::resolve_workspace(workspace)?;
    let roots = paths::resolve_roots(readonly_roots, &workspace)?;
    let command = quote::command_line(&executable, args);
    Ok(policy::request(
        command,
        &roots,
        &workspace,
        env::host_home().as_deref(),
        proxy,
    ))
}

fn resolve_executable(executable: &Path) -> Result<PathBuf> {
    let target = paths::canonicalize(executable, "executable")?;
    paths::require_file(&target)?;
    paths::launcher_path(executable)
}

fn execute(request: ContainerRequest) -> Result<String> {
    preflight()?;
    finish(invoke(request)?)
}

fn invoke(request: ContainerRequest) -> Result<ExecutionResult> {
    v1::run(request, Default::default()).context("MXC execution failed")
}

fn preflight() -> Result<()> {
    let support = v1::platform_support();
    if !support.is_supported
        || !support
            .available_methods
            .iter()
            .any(|method| method == "seatbelt")
    {
        bail!("Seatbelt is unavailable: {support:?}");
    }
    Ok(())
}

fn finish(result: ExecutionResult) -> Result<String> {
    interpret_outcome(
        result.outcome,
        format_output(&result.stdout, &result.stderr),
    )
}

const SUCCESS_EXIT: i32 = 0;

fn interpret_outcome(outcome: WaitResult, output: String) -> Result<String> {
    match outcome {
        WaitResult::Exited(SUCCESS_EXIT) => Ok(output),
        outcome => bail!("contained command failed with {outcome:?}: {output}"),
    }
}
