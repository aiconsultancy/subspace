//! Runtime-specific installation operations.
use super::{EnvironmentSpec, RuntimeKind, fs, link_runtime, run_mise, write_state};
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) fn lock_and_install(mise: &Path, root: &Path, project: &Path) -> Result<()> {
    run_mise(mise, root, project, &["lock", "--platform", "macos-arm64"])?;
    run_mise(mise, root, project, &["install", "--yes", "--locked"])?;
    Ok(())
}

pub(super) fn write_runtime_version(project: &Path, spec: &EnvironmentSpec) -> Result<()> {
    match spec.runtime {
        RuntimeKind::Python => fs::write(
            project.join(".python-version"),
            format!("{}\n", spec.version),
        )?,
        RuntimeKind::Node => {
            fs::write(project.join(".node-version"), format!("{}\n", spec.version))?
        }
        RuntimeKind::Bun => {
            let path = project.join("package.json");
            let mut value: Value = serde_json::from_slice(&fs::read(&path)?)?;
            value["packageManager"] = Value::String(format!("bun@{}", spec.version));
            fs::write(path, serde_json::to_vec_pretty(&value)?)?;
        }
        RuntimeKind::Shell => {}
    }
    Ok(())
}

pub(super) fn install_python(
    root: &Path,
    project: &Path,
    spec: &EnvironmentSpec,
    mise: &Path,
) -> Result<()> {
    let python = run_mise(mise, root, project, &["which", "python"])?;
    run_mise(
        mise,
        root,
        project,
        &[
            "exec",
            "--",
            "uv",
            "sync",
            "--no-build",
            "--python",
            python.trim(),
        ],
    )?;
    let executable = project.join(".venv/bin/python");
    ensure!(
        executable.is_file(),
        "uv sync did not create project interpreter"
    );
    write_state(project, spec, &executable)
}

pub(super) fn install_javascript(
    root: &Path,
    project: &Path,
    spec: &EnvironmentSpec,
    mise: &Path,
) -> Result<()> {
    let (manager, executable) = match spec.runtime {
        RuntimeKind::Node => ("pnpm", "node"),
        RuntimeKind::Bun => ("bun", "bun"),
        _ => bail!("JavaScript installer received a non-JavaScript runtime"),
    };
    run_mise(
        mise,
        root,
        project,
        &["exec", "--", manager, "install", "--no-scripts"],
    )?;
    let executable_path =
        PathBuf::from(run_mise(mise, root, project, &["which", executable])?.trim());
    link_runtime(project, executable, &executable_path)?;
    write_state(
        project,
        spec,
        &project.join(format!(".mise-bin/{executable}")),
    )
}

pub(super) fn install_shell(
    root: &Path,
    project: &Path,
    spec: &EnvironmentSpec,
    mise: &Path,
) -> Result<()> {
    let jq = PathBuf::from(run_mise(mise, root, project, &["which", "jq"])?.trim());
    link_runtime(project, "jq", &jq)?;
    fs::write(
        project.join("shell-project.json"),
        "{\"kind\":\"jq-filter\"}\n",
    )?;
    write_state(project, spec, &project.join(".mise-bin/jq"))
}
