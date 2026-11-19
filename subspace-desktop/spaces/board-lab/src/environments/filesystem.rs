//! Environment project manifests, state, and filesystem operations.
use super::{BUN, EnvironmentSpec, PNPM, ProjectState, RuntimeKind, UV};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::fs;
use std::path::Path;
fn tool_config(spec: &EnvironmentSpec) -> String {
    let runtime = match spec.runtime {
        RuntimeKind::Python => format!("python = \"{}\"", spec.version),
        RuntimeKind::Node => format!("node = \"{}\"", spec.version),
        RuntimeKind::Bun => format!("bun = \"{}\"", spec.version),
        RuntimeKind::Shell => format!("jq = \"{}\"", spec.version),
    };
    let extras = match spec.runtime {
        RuntimeKind::Python => format!("\nuv = \"{UV}\""),
        RuntimeKind::Node => format!("\npnpm = \"{PNPM}\""),
        RuntimeKind::Bun => String::new(),
        RuntimeKind::Shell => String::new(),
    };
    let settings = if matches!(spec.runtime, RuntimeKind::Python) {
        "\n[settings]\npython.github_attestations = false\n"
    } else {
        ""
    };
    format!("[tools]\n{runtime}{extras}\n{settings}")
}
pub(super) fn write_project(project: &Path, spec: &EnvironmentSpec) -> Result<()> {
    fs::write(project.join(".mise.toml"), tool_config(spec))?;
    match spec.runtime {
        RuntimeKind::Python => fs::write(
            project.join("pyproject.toml"),
            format!(
                "[project]\nname = \"{}\"\nversion = \"0.0.0\"\nrequires-python = \">={}\"\ndependencies = [{}]\n",
                spec.id,
                spec.version,
                spec.packages
                    .iter()
                    .map(|p| format!("\"{p}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )?,
        RuntimeKind::Node => {
            let deps = spec
                .packages
                .iter()
                .map(|p| {
                    let (name, version) = p.rsplit_once('@').unwrap_or((p, "latest"));
                    format!("\"{}\":\"{}\"", json_escape(name), json_escape(version))
                })
                .collect::<Vec<_>>()
                .join(",");
            fs::write(
                project.join("package.json"),
                format!(
                    "{{\"name\":\"{}\",\"private\":true,\"packageManager\":\"pnpm@{PNPM}\",\"dependencies\":{{{deps}}}}}\n",
                    json_escape(&spec.id)
                ),
            )?;
        }
        RuntimeKind::Bun => fs::write(
            project.join("package.json"),
            format!(
                "{{\"name\":\"{}\",\"private\":true,\"packageManager\":\"bun@{BUN}\",\"dependencies\":{{}}}}\n",
                json_escape(&spec.id)
            ),
        )?,
        RuntimeKind::Shell => {}
    }
    Ok(())
}
pub(super) fn valid_project(project: &Path, runtime: &RuntimeKind) -> bool {
    if matches!(runtime, RuntimeKind::Python) {
        valid_python(project)
    } else if matches!(runtime, RuntimeKind::Node) {
        valid_node(project)
    } else if matches!(runtime, RuntimeKind::Bun) {
        valid_bun(project)
    } else {
        valid_shell(project)
    }
}

fn valid_python(project: &Path) -> bool {
    project.join("pyproject.toml").is_file()
        && project.join("uv.lock").is_file()
        && project.join(".venv/bin/python").is_file()
}

fn valid_node(project: &Path) -> bool {
    project.join("package.json").is_file()
        && project.join("pnpm-lock.yaml").is_file()
        && project.join("node_modules").is_dir()
}

fn valid_bun(project: &Path) -> bool {
    project.join("package.json").is_file()
        && (project.join("bun.lock").is_file() || empty_bun_dependencies(project))
}

fn empty_bun_dependencies(project: &Path) -> bool {
    fs::read(project.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("dependencies")
                .and_then(Value::as_object)
                .map(|deps| deps.is_empty())
        })
        .unwrap_or(false)
}

fn valid_shell(project: &Path) -> bool {
    project.join(".mise.toml").is_file() && project.join("shell-project.json").is_file()
}
pub(super) fn write_state(project: &Path, spec: &EnvironmentSpec, executable: &Path) -> Result<()> {
    fs::write(
        project.join("environment.json"),
        serde_json::to_vec(&ProjectState {
            version: spec.version.clone(),
            executable: Some(executable.to_owned()),
        })?,
    )?;
    Ok(())
}
pub(super) fn read_state(project: &Path) -> Result<ProjectState> {
    Ok(serde_json::from_slice(&fs::read(
        project.join("environment.json"),
    )?)?)
}
pub(super) fn link_runtime(project: &Path, name: &str, target: &Path) -> Result<()> {
    if !target.is_file() {
        bail!("mise did not resolve {name}")
    }
    let dir = project.join(".mise-bin");
    fs::create_dir_all(&dir)?;
    let link = dir.join(name);
    let _ = fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, &link)?;
    #[cfg(not(unix))]
    fs::copy(target, &link)?;
    Ok(())
}
pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        copy_entry(&from, &to)?;
    }
    Ok(())
}

fn copy_entry(source: &Path, destination: &Path) -> Result<()> {
    if excluded_tree(source) {
        return Ok(());
    }
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        copy_tree(source, destination)
    } else {
        fs::copy(source, destination)?;
        Ok(())
    }
}

fn excluded_tree(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == ".venv" || name == "node_modules" || name == ".mise-bin")
}
pub(super) fn add_python_package(path: &Path, package: &str) -> Result<()> {
    let mut text = fs::read_to_string(path)?;
    let escaped = package.replace('"', "\\\"");
    let entry = format!("\"{escaped}\"");
    if text.lines().any(|line| line.contains(&entry)) {
        bail!("package already present")
    }
    let marker = "dependencies = [";
    let position = text.find(marker).context("dependencies section missing")? + marker.len();
    text.insert_str(position, &format!("\n  {entry},"));
    fs::write(path, text)?;
    Ok(())
}
pub(super) fn add_node_package(path: &Path, package: &str) -> Result<()> {
    let (name, version) = package
        .rfind('@')
        .filter(|index| *index > 0)
        .map(|index| (&package[..index], &package[index + 1..]))
        .unwrap_or((package, "latest"));
    let mut value: Value = serde_json::from_slice(&fs::read(path)?)?;
    let deps = value
        .get_mut("dependencies")
        .and_then(Value::as_object_mut)
        .context("dependencies section missing")?;
    if deps.contains_key(name) {
        bail!("package already present")
    }
    deps.insert(name.to_owned(), Value::String(version.to_owned()));
    fs::write(path, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}
fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
