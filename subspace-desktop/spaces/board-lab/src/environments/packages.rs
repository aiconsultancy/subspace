//! Atomic package updates for provisioned environments.

use super::{
    EnvironmentInfo, EnvironmentSpec, RuntimeKind, add_node_package, add_python_package, copy_tree,
};
use anyhow::{Result, bail};
use std::fs;

pub(super) fn add_package(
    root: &std::path::Path,
    id: &str,
    package: &str,
) -> Result<EnvironmentInfo> {
    super::validate_package(package)?;
    let spec = super::catalog::spec(root, id)?;
    let project = super::catalog::project(root, id);
    ensure_ready(&project, id)?;
    let stage = super::provisioning::stage_path(root, id)?;
    prepare_stage(&project, &stage, &spec.runtime, package)?;
    install_and_activate(root, &stage, &project, &spec)?;
    super::catalog::info(root, spec)
}

fn prepare_stage(
    project: &std::path::Path,
    stage: &std::path::Path,
    runtime: &RuntimeKind,
    package: &str,
) -> Result<()> {
    copy_tree(project, stage)?;
    update_manifest(runtime, stage, package)
}

fn install_and_activate(
    root: &std::path::Path,
    stage: &std::path::Path,
    project: &std::path::Path,
    spec: &EnvironmentSpec,
) -> Result<()> {
    if let Err(error) = super::provisioning::install_project(root, stage, spec) {
        let _ = fs::remove_dir_all(stage);
        return Err(error.context("package update failed; active environment retained"));
    }
    super::provisioning::activate_revision(stage, project)
}

fn ensure_ready(project: &std::path::Path, id: &str) -> Result<()> {
    if !project.join(".ready").is_file() {
        bail!("environment {id} is not ready");
    }
    Ok(())
}

fn update_manifest(runtime: &RuntimeKind, stage: &std::path::Path, package: &str) -> Result<()> {
    match runtime {
        RuntimeKind::Python => add_python_package(&stage.join("pyproject.toml"), package),
        RuntimeKind::Node | RuntimeKind::Bun => {
            add_node_package(&stage.join("package.json"), package)
        }
        RuntimeKind::Shell => bail!("cli-lab has no package manager"),
    }
}
