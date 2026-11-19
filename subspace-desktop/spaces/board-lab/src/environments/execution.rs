//! Contained source execution and disposable job workspaces.

use super::{EnvironmentInfo, RuntimeKind};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SOURCE_BYTES: usize = 128 * 1024;

struct ExecutionPlan {
    executable: PathBuf,
    args: Vec<String>,
    filename: PathBuf,
}

pub(super) fn run_source(
    root: &Path,
    id: &str,
    source: &str,
    input: &Value,
    proxy: Option<&str>,
) -> Result<String> {
    validate_source(source)?;
    let spec = super::catalog::spec(root, id)?;
    let info = ready_info(root, &spec, id)?;
    let job = super::provisioning::unique_job(root, id)?;
    write_input(&job, input)?;
    let plan = prepare_plan(&spec.runtime, &info, &job, source)?;
    let roots = readonly_roots(root, id, &info, &plan.executable)?;
    mxc_probe::run_scoped(
        &plan.executable,
        &plan.args.iter().map(String::as_str).collect::<Vec<_>>(),
        &roots,
        &job,
        proxy,
    )
    .with_context(|| format!("contained execution of {}", plan.filename.display()))
}

fn ready_info(root: &Path, spec: &super::EnvironmentSpec, id: &str) -> Result<EnvironmentInfo> {
    let info = super::catalog::info(root, spec.clone())?;
    if !info.ready {
        bail!("environment {id} is not ready; provision it first")
    }
    Ok(info)
}

fn readonly_roots(
    root: &Path,
    id: &str,
    info: &EnvironmentInfo,
    executable: &Path,
) -> Result<Vec<PathBuf>> {
    let mut roots = vec![info.project.clone()];
    add_tool_roots(root, &info.project, &mut roots)?;
    add_executable_root(root, id, executable, &mut roots)?;
    Ok(roots)
}

fn validate_source(source: &str) -> Result<()> {
    if source.len() > MAX_SOURCE_BYTES {
        bail!("source exceeds 128 KiB limit")
    }
    Ok(())
}

fn write_input(job: &Path, input: &Value) -> Result<()> {
    fs::write(job.join("input.json"), serde_json::to_vec(input)?)?;
    Ok(())
}

fn prepare_plan(
    runtime: &RuntimeKind,
    info: &EnvironmentInfo,
    job: &Path,
    source: &str,
) -> Result<ExecutionPlan> {
    let (filename, args) = match runtime {
        RuntimeKind::Python => write_script(job, "source.py", source, vec!["-I"]),
        RuntimeKind::Node => write_script(job, "source.js", source, Vec::new()),
        RuntimeKind::Bun => write_script(job, "source.ts", source, Vec::new()),
        RuntimeKind::Shell => write_shell(job, source),
    }?;
    let mut args = args;
    args.push(filename.to_string_lossy().into_owned());
    if matches!(runtime, RuntimeKind::Shell) {
        args.push(job.join("input.json").to_string_lossy().into_owned());
    }
    #[cfg(unix)]
    if matches!(runtime, RuntimeKind::Node | RuntimeKind::Bun)
        && info.project.join("node_modules").is_dir()
    {
        std::os::unix::fs::symlink(info.project.join("node_modules"), job.join("node_modules"))?;
    }
    Ok(ExecutionPlan {
        executable: info.executable.clone(),
        args,
        filename,
    })
}

fn write_script(
    job: &Path,
    name: &str,
    source: &str,
    args: Vec<&str>,
) -> Result<(PathBuf, Vec<String>)> {
    let file = job.join(name);
    fs::write(&file, source)?;
    Ok((file, args.into_iter().map(str::to_owned).collect()))
}

fn write_shell(job: &Path, source: &str) -> Result<(PathBuf, Vec<String>)> {
    write_script(job, "filter.jq", source, vec!["-f"])
}

fn add_tool_roots(root: &Path, project: &Path, roots: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(bytes) = fs::read(project.join("tool-roots.json")) else {
        return Ok(());
    };
    let paths: Vec<PathBuf> = serde_json::from_slice(&bytes)?;
    if paths.is_empty() {
        return Ok(());
    }
    let installs = fs::canonicalize(root.join("mise-data/installs"))?;
    for path in paths {
        let path = fs::canonicalize(path)?;
        if !path.starts_with(&installs) {
            bail!("Extra tool path is outside private installations");
        }
        if !roots.contains(&path) {
            roots.push(path);
        }
    }
    Ok(())
}

fn add_executable_root(
    root: &Path,
    id: &str,
    executable: &Path,
    roots: &mut Vec<PathBuf>,
) -> Result<()> {
    let resolved = fs::canonicalize(executable).unwrap_or_else(|_| executable.to_owned());
    let Some(parent) = resolved.parent() else {
        return Ok(());
    };
    if super::catalog::project(root, id).starts_with(parent) {
        return Ok(());
    }
    let root = if parent.file_name().is_some_and(|name| name == "bin") {
        parent
            .parent()
            .context("Interpreter bin has no install root")?
            .to_owned()
    } else {
        parent.to_owned()
    };
    if !roots.contains(&root) {
        roots.push(root);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::EnvironmentManager;
    use super::{add_executable_root, add_tool_roots, prepare_plan, validate_source};
    use crate::contracts::{EnvironmentInfo, EnvironmentSpec, RuntimeKind};
    use serde_json::json;
    use std::path::PathBuf;

    fn info(project: PathBuf) -> EnvironmentInfo {
        EnvironmentInfo {
            spec: EnvironmentSpec {
                id: "python-data".into(),
                title: "Python data".into(),
                runtime: RuntimeKind::Python,
                version: "3.12.8".into(),
                package_manager: "uv".into(),
                packages: Vec::new(),
                tools: Vec::new(),
            },
            ready: true,
            executable: project.join(".venv/bin/python"),
            project,
            summary: "ready".into(),
        }
    }

    #[test]
    fn source_plan_writes_python_file_and_input_shape() {
        let directory = tempfile::tempdir().expect("temporary execution root");
        let job = directory.path().join("job");
        std::fs::create_dir(&job).expect("job directory");
        let environment = info(directory.path().join("project"));
        let plan = prepare_plan(&RuntimeKind::Python, &environment, &job, "print(1)")
            .expect("python plan");
        assert!(plan.filename.ends_with("source.py"));
        assert_eq!(
            plan.args,
            vec![
                "-I".to_owned(),
                plan.filename.to_string_lossy().into_owned()
            ]
        );
        assert_eq!(
            std::fs::read_to_string(plan.filename).expect("source"),
            "print(1)"
        );
        validate_source("small").expect("small source");
        assert!(validate_source(&"x".repeat(128 * 1024 + 1)).is_err());
        std::fs::create_dir_all(&environment.project).expect("project directory");
        std::fs::write(
            environment.project.join("tool-roots.json"),
            serde_json::to_vec(&Vec::<PathBuf>::new()).unwrap(),
        )
        .expect("tool roots");
        add_tool_roots(directory.path(), &environment.project, &mut Vec::new())
            .expect("empty tool roots");
    }

    #[test]
    fn missing_environment_fails_before_worker_start() {
        let directory = tempfile::tempdir().expect("temporary execution root");
        let manager = EnvironmentManager::new(directory.path().join("board-lab"));
        assert!(
            manager
                .run_source("missing", "print(1)", &json!({}))
                .is_err()
        );
        let mut roots = Vec::new();
        let environment = info(directory.path().join("project"));
        add_executable_root(
            &manager.root,
            "python-data",
            &environment.executable,
            &mut roots,
        )
        .expect("executable root");
        assert_eq!(roots.len(), 1);
    }
}
