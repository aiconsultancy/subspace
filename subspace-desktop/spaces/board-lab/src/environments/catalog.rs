//! Catalog, project inspection, and environment metadata.

use super::{
    BUN, EnvironmentInfo, EnvironmentSpec, JQ, NODE, PYTHON, PYTHON_ALT, RuntimeKind, fs,
    read_state, valid_project, validate_spec,
};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};

struct EnvironmentTemplate<'a> {
    id: &'a str,
    title: &'a str,
    runtime: RuntimeKind,
    version: &'a str,
    package_manager: &'a str,
    packages: &'a [&'a str],
    tools: &'a [&'a str],
}

fn default_templates() -> Vec<EnvironmentSpec> {
    vec![
        template(EnvironmentTemplate {
            id: "python-data",
            title: "Python data",
            runtime: RuntimeKind::Python,
            version: PYTHON,
            package_manager: "uv",
            packages: &["polars==1.17.1", "pandas==2.2.3"],
            tools: &[],
        }),
        template(EnvironmentTemplate {
            id: "node-services",
            title: "Node services",
            runtime: RuntimeKind::Node,
            version: NODE,
            package_manager: "pnpm",
            packages: &["semver@7.7.4"],
            tools: &[],
        }),
        template(EnvironmentTemplate {
            id: "bun-notebook",
            title: "Bun notebook",
            runtime: RuntimeKind::Bun,
            version: BUN,
            package_manager: "bun",
            packages: &[],
            tools: &[],
        }),
        template(EnvironmentTemplate {
            id: "python-alt",
            title: "Python alternate",
            runtime: RuntimeKind::Python,
            version: PYTHON_ALT,
            package_manager: "uv",
            packages: &["packaging==24.2"],
            tools: &[],
        }),
        template(EnvironmentTemplate {
            id: "cli-lab",
            title: "CLI lab",
            runtime: RuntimeKind::Shell,
            version: JQ,
            package_manager: "mise",
            packages: &[],
            tools: &["jq"],
        }),
    ]
}

pub(super) fn specs(root: &Path) -> Result<Vec<EnvironmentSpec>> {
    let path = root.join("catalog.json");
    let mut specs = if path.exists() {
        let bytes = fs::read(path)?;
        if bytes.len() > super::MAX_CATALOG_BYTES {
            bail!("Environment catalog exceeds 64 KiB");
        }
        serde_json::from_slice::<Vec<EnvironmentSpec>>(&bytes)?
    } else {
        let mut defaults = default_templates();
        for spec in &mut defaults {
            if let Ok(state) = read_state(&project(root, &spec.id)) {
                spec.version = state.version;
            }
        }
        defaults
    };
    if specs.len() > super::MAX_ENVIRONMENTS {
        bail!("Too many environments");
    }
    let mut ids = std::collections::HashSet::new();
    for spec in &mut specs {
        validate_spec(spec)?;
        if !ids.insert(spec.id.clone()) {
            bail!("Duplicate environment ID");
        }
    }
    Ok(specs)
}

pub(super) fn spec(root: &Path, id: &str) -> Result<EnvironmentSpec> {
    let mut spec = specs(root)?
        .into_iter()
        .find(|s| s.id == id)
        .with_context(|| format!("unknown environment id: {id}"))?;
    if project(root, id).join("environment.json").exists() {
        spec.version = read_state(&project(root, id))?.version;
    }
    Ok(spec)
}
pub(super) fn project(root: &Path, id: &str) -> PathBuf {
    root.join("projects").join(id)
}
pub(super) fn info(root: &Path, mut spec: EnvironmentSpec) -> Result<EnvironmentInfo> {
    let alias = project(root, &spec.id);
    let project = fs::canonicalize(&alias).unwrap_or(alias);
    spec.tools = manifest_tools(&project);
    spec.packages = package_list(&project, &spec.runtime, &spec.packages);
    let executable = executable_path(&project, &spec.runtime, read_state(&project).ok());
    let ready = project.join(".ready").is_file()
        && executable.is_file()
        && valid_project(&project, &spec.runtime);
    let summary = summary(root, &spec, ready)?;
    Ok(EnvironmentInfo {
        spec,
        ready,
        project,
        executable,
        summary,
    })
}

fn summary(root: &Path, spec: &EnvironmentSpec, ready: bool) -> Result<String> {
    if !ready {
        return Ok("not provisioned".into());
    }
    let desired = specs(root)?
        .into_iter()
        .find(|configured| configured.id == spec.id)
        .map(|configured| configured.version)
        .unwrap_or_else(|| spec.version.clone());
    Ok(if desired == spec.version {
        "ready (private mise project)".into()
    } else {
        format!(
            "Active {} retained; config requests {desired}. Provision to activate it.",
            spec.version
        )
    })
}

pub(super) fn write_specs(root: &Path, specs: &[EnvironmentSpec]) -> Result<()> {
    fs::create_dir_all(root)?;
    let file = root.join("catalog.json");
    let temporary = root.join(format!("catalog-{}.tmp", std::process::id()));
    fs::write(&temporary, serde_json::to_vec_pretty(specs)?)?;
    fs::rename(temporary, file)?;
    Ok(())
}

fn template(template: EnvironmentTemplate<'_>) -> EnvironmentSpec {
    EnvironmentSpec {
        id: template.id.into(),
        title: template.title.into(),
        runtime: template.runtime,
        version: template.version.into(),
        package_manager: template.package_manager.into(),
        packages: template
            .packages
            .iter()
            .map(|value| (*value).into())
            .collect(),
        tools: template.tools.iter().map(|value| (*value).into()).collect(),
    }
}

fn manifest_tools(project: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(project.join(".mise.toml")) else {
        return Vec::new();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };
    value
        .get("tools")
        .and_then(toml::Value::as_table)
        .map(|tools| {
            tools
                .iter()
                .map(|(name, version)| format!("{name}@{}", version.as_str().unwrap_or("?")))
                .collect()
        })
        .unwrap_or_default()
}

fn package_list(project: &Path, runtime: &RuntimeKind, fallback: &[String]) -> Vec<String> {
    match runtime {
        RuntimeKind::Python => python_packages(project).unwrap_or_else(|| fallback.to_vec()),
        RuntimeKind::Node | RuntimeKind::Bun => {
            node_packages(project).unwrap_or_else(|| fallback.to_vec())
        }
        RuntimeKind::Shell => fallback.to_vec(),
    }
}

fn python_packages(project: &Path) -> Option<Vec<String>> {
    let value =
        toml::from_str::<toml::Value>(&fs::read_to_string(project.join("pyproject.toml")).ok()?)
            .ok()?;
    value
        .get("project")
        .and_then(|project| project.get("dependencies"))
        .and_then(toml::Value::as_array)
        .map(|packages| {
            packages
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
}

fn node_packages(project: &Path) -> Option<Vec<String>> {
    let value: Value =
        serde_json::from_slice(&fs::read(project.join("package.json")).ok()?).ok()?;
    value
        .get("dependencies")
        .and_then(Value::as_object)
        .map(|packages| {
            packages
                .iter()
                .map(|(name, version)| format!("{name}@{}", version.as_str().unwrap_or("?")))
                .collect()
        })
}

fn executable_path(
    project: &Path,
    runtime: &RuntimeKind,
    state: Option<super::ProjectState>,
) -> PathBuf {
    state
        .and_then(|state| state.executable)
        .unwrap_or_else(|| project.join(super::runtime_layout(runtime).executable))
}

#[cfg(test)]
mod tests {
    use super::super::EnvironmentManager;
    use super::{EnvironmentSpec, RuntimeKind, executable_path, manifest_tools, project, spec};
    use std::path::Path;

    #[test]
    fn catalog_defaults_are_readable_without_creating_storage() {
        let directory = tempfile::tempdir().expect("temporary catalog root");
        let manager = EnvironmentManager::new(directory.path().join("board-lab"));
        let catalog = manager.catalog().expect("default catalog");
        assert_eq!(catalog.len(), 5);
        assert!(!directory.path().join("board-lab").exists());
    }

    #[test]
    fn registration_persists_a_valid_custom_spec() {
        let directory = tempfile::tempdir().expect("temporary catalog root");
        let manager = EnvironmentManager::new(directory.path().join("board-lab"));
        let custom = EnvironmentSpec {
            id: "custom-python".into(),
            title: "Custom Python".into(),
            runtime: RuntimeKind::Python,
            version: "3.12.8".into(),
            package_manager: "uv".into(),
            packages: vec!["polars==1.17.1".into()],
            tools: Vec::new(),
        };
        manager
            .register(custom)
            .expect("register custom environment");
        assert!(spec(&manager.root, "custom-python").is_ok());
        assert!(manager.root.join("catalog.json").is_file());
        assert!(!project(&manager.root, "custom-python").exists());
    }

    #[test]
    fn manifest_and_executable_helpers_preserve_expected_paths() {
        let directory = tempfile::tempdir().expect("temporary project");
        std::fs::write(
            directory.path().join(".mise.toml"),
            "[tools]\npython = \"3.12.8\"\n",
        )
        .expect("write mise manifest");
        assert_eq!(manifest_tools(directory.path()), vec!["python@3.12.8"]);
        assert_eq!(
            executable_path(directory.path(), &RuntimeKind::Python, None),
            Path::new(directory.path()).join(".venv/bin/python")
        );
    }
}
