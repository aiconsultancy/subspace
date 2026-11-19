//! Explicit provisioning, immutable revisions, and atomic activation.

use super::{
    EnvironmentInfo, EnvironmentSpec, MAX_JOB_ATTEMPTS, RuntimeKind, bundled_mise, copy_tree,
    extensions, fs, runtime_tool_name, valid_project, verify_mise, which, write_project,
};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) use super::installers::{
    install_javascript, install_python, install_shell, lock_and_install, write_runtime_version,
};

pub(super) fn bootstrap_mise(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("bin"))?;
    let target = root.join("bin/mise");
    if target.is_file() {
        verify_mise(&target)?;
    } else {
        let bundled = bundled_mise();
        let source = if cfg!(debug_assertions) {
            bundled
                .or_else(|| which("mise"))
                .context("No bundled mise executable and no developer mise found on PATH")?
        } else {
            bundled.context("Release Board Lab requires bundled mise")?
        };
        fs::copy(&source, &target)?;
        verify_mise(&target)?;
        fs::write(
            root.join("bootstrap-source.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"source":source,"private_executable":target,"note":"Developer-built vendor copy, not a release bootstrap installer."}),
            )?,
        )?;
    }
    for dir in [
        "mise-config",
        "mise-data",
        "mise-cache",
        "mise-state",
        "private-home",
        "tmp",
    ] {
        fs::create_dir_all(root.join(dir))?;
    }
    let global_config = root.join("mise-config/config.toml");
    if !global_config.is_file() {
        fs::write(global_config, "# empty private global config\n")?;
    }
    Ok(())
}

pub(super) fn provision(root: &Path, id: &str) -> Result<EnvironmentInfo> {
    let configured = super::catalog::specs(root)?
        .into_iter()
        .find(|spec| spec.id == id)
        .context("Unknown environment")?;
    let mut spec = super::catalog::info(root, super::catalog::spec(root, id)?)?.spec;
    spec.version = configured.version;
    bootstrap_mise(root)?;
    replace_with_provisioned(root, &spec)
}

pub(super) fn set_version(root: &Path, id: &str, version: &str) -> Result<EnvironmentInfo> {
    super::validate_version(version)?;
    let mut spec = super::catalog::info(root, super::catalog::spec(root, id)?)?.spec;
    spec.version = version.to_owned();
    bootstrap_mise(root)?;
    replace_with_provisioned(root, &spec)?;
    let mut specs = super::catalog::specs(root)?;
    if let Some(configured) = specs.iter_mut().find(|configured| configured.id == id) {
        configured.version = version.into();
    }
    super::catalog::write_specs(root, &specs)?;
    super::catalog::info(root, spec)
}
pub(super) fn replace_with_provisioned(
    root: &Path,
    spec: &EnvironmentSpec,
) -> Result<EnvironmentInfo> {
    let stage = stage_path(root, &spec.id)?;
    let project = super::catalog::project(root, &spec.id);
    stage_provisioned_project(&project, &stage, spec)?;
    if let Err(error) = install_project(root, &stage, spec) {
        let _ = fs::remove_dir_all(&stage);
        return Err(error.context("provisioning failed; active environment retained"));
    }
    activate_revision(&stage, &project)?;
    super::catalog::info(root, spec.clone())
}

fn stage_provisioned_project(project: &Path, stage: &Path, spec: &EnvironmentSpec) -> Result<()> {
    if project.is_dir() {
        copy_tree(project, stage)?;
        update_runtime_manifest(stage, spec)?;
    } else {
        write_project(stage, spec)?;
    }
    Ok(())
}

fn update_runtime_manifest(stage: &Path, spec: &EnvironmentSpec) -> Result<()> {
    let path = stage.join(".mise.toml");
    let mut value: toml::Value = toml::from_str(&fs::read_to_string(&path)?)?;
    value
        .get_mut("tools")
        .and_then(toml::Value::as_table_mut)
        .context("Project has no tools table")?
        .insert(
            runtime_tool_name(&spec.runtime).into(),
            toml::Value::String(spec.version.clone()),
        );
    fs::write(path, toml::to_string_pretty(&value)?)?;
    Ok(())
}
pub(super) fn install_project(root: &Path, project: &Path, spec: &EnvironmentSpec) -> Result<()> {
    let mise = root.join("bin/mise");
    lock_and_install(&mise, root, project)?;
    write_runtime_version(project, spec)?;
    install_runtime(root, project, spec, &mise)?;
    extensions::record_tool_paths(root, project)?;
    if !valid_project(project, &spec.runtime) {
        bail!("Package manager did not produce a usable project and lock state");
    }
    fs::write(project.join(".ready"), "provisioned\n")?;
    Ok(())
}

fn install_runtime(root: &Path, project: &Path, spec: &EnvironmentSpec, mise: &Path) -> Result<()> {
    match spec.runtime {
        RuntimeKind::Python => install_python(root, project, spec, mise),
        RuntimeKind::Node | RuntimeKind::Bun => install_javascript(root, project, spec, mise),
        RuntimeKind::Shell => install_shell(root, project, spec, mise),
    }
}
pub(super) fn stage_path(root: &Path, id: &str) -> Result<PathBuf> {
    fs::create_dir_all(root.join("projects"))?;
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = root.join("projects").join(format!(".{id}.revision-{n}"));
    fs::create_dir(&path)?;
    Ok(path)
}
pub(super) fn unique_job(root: &Path, id: &str) -> Result<PathBuf> {
    let root = root.join("jobs");
    fs::create_dir_all(&root)?;
    for attempt in 0..MAX_JOB_ATTEMPTS {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = root.join(format!("{id}-{n}-{attempt}"));
        if fs::create_dir(&path).is_ok() {
            return Ok(path);
        }
    }
    bail!("could not allocate job workspace")
}

pub(super) fn activate_revision(stage: &Path, project: &Path) -> Result<()> {
    let temporary = make_activation_link(stage, project)?;
    retire_active(project)?;
    fs::rename(temporary, project)?;
    Ok(())
}

#[cfg(unix)]
fn make_activation_link(stage: &Path, project: &Path) -> Result<PathBuf> {
    let temporary = project.with_extension(format!("next-{}", std::process::id()));
    let _ = fs::remove_file(&temporary);
    std::os::unix::fs::symlink(stage, &temporary)?;
    Ok(temporary)
}

#[cfg(not(unix))]
fn make_activation_link(_stage: &Path, _project: &Path) -> Result<PathBuf> {
    bail!("Board Lab revision activation requires macOS/Unix")
}

fn retire_active(project: &Path) -> Result<()> {
    if fs::symlink_metadata(project).is_ok_and(|metadata| !metadata.file_type().is_symlink()) {
        let legacy = project.with_extension(format!("legacy-{}", super::timestamp_nanos()));
        fs::rename(project, legacy)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::EnvironmentManager;
    use super::{
        EnvironmentSpec, RuntimeKind, activate_revision, install_javascript, install_project,
        install_python, install_shell, lock_and_install, stage_path, unique_job,
        write_runtime_version,
    };
    use std::path::Path;

    fn spec(runtime: RuntimeKind) -> EnvironmentSpec {
        EnvironmentSpec {
            id: "test".into(),
            title: "Test".into(),
            runtime,
            version: "3.12.8".into(),
            package_manager: "uv".into(),
            packages: Vec::new(),
            tools: Vec::new(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn activation_switches_only_the_alias_and_retains_revision() {
        let directory = tempfile::tempdir().expect("temporary activation root");
        let stage = directory.path().join("revision");
        let project = directory.path().join("project");
        std::fs::create_dir_all(&stage).expect("revision directory");
        std::fs::write(stage.join("marker"), "new").expect("revision marker");
        std::fs::create_dir_all(&project).expect("active directory");
        std::fs::write(project.join("marker"), "old").expect("active marker");
        activate_revision(&stage, &project).expect("activate revision");
        assert!(
            std::fs::symlink_metadata(&project)
                .expect("active alias")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(project.join("marker")).expect("new marker"),
            "new"
        );
        assert!(stage.join("marker").is_file());
    }

    #[cfg(not(unix))]
    #[test]
    fn unsupported_activation_preserves_existing_project() {
        let directory = tempfile::tempdir().expect("temporary activation root");
        let project = directory.path().join("project");
        std::fs::create_dir(&project).expect("active directory");
        let marker = project.join("marker");
        std::fs::write(&marker, "existing").expect("active marker");
        assert!(activate_revision(&directory.path().join("revision"), &project).is_err());
        assert_eq!(
            std::fs::read_to_string(marker).expect("retained marker"),
            "existing"
        );
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("unchanged root")
                .count(),
            1
        );
    }

    #[test]
    fn staging_and_runtime_manifest_updates_are_local_filesystem_operations() {
        let directory = tempfile::tempdir().expect("temporary provisioning root");
        let manager = EnvironmentManager::new(directory.path().join("board-lab"));
        let stage = stage_path(&manager.root, "python-data").expect("stage path");
        assert!(stage.starts_with(manager.root.join("projects")));
        let job = unique_job(&manager.root, "python-data").expect("job path");
        assert!(job.starts_with(manager.root.join("jobs")));
        write_runtime_version(&stage, &spec(RuntimeKind::Python)).expect("python version");
        assert_eq!(
            std::fs::read_to_string(stage.join(".python-version")).expect("version file"),
            "3.12.8\n"
        );
    }

    #[test]
    fn installers_fail_closed_before_any_external_download() {
        let directory = tempfile::tempdir().expect("temporary provisioning root");
        let project = directory.path().join("project");
        std::fs::create_dir_all(&project).expect("project directory");
        let manager = EnvironmentManager::new(directory.path().join("board-lab"));
        let mise = Path::new("/definitely-missing/board-lab-mise");
        assert!(lock_and_install(mise, &manager.root, &project).is_err());
        assert!(install_project(&manager.root, &project, &spec(RuntimeKind::Python)).is_err());
        assert!(install_python(&manager.root, &project, &spec(RuntimeKind::Python), mise).is_err());
        assert!(
            install_javascript(&manager.root, &project, &spec(RuntimeKind::Node), mise).is_err()
        );
        assert!(install_shell(&manager.root, &project, &spec(RuntimeKind::Shell), mise).is_err());
    }
}
