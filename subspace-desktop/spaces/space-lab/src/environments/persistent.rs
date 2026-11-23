//! Persistent-only preparation. The one-shot execution path is unchanged.

use crate::project::SourceSnapshot;
use anyhow::{Context, Result, bail, ensure};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct PreparedRuntime {
    pub(crate) request: mxc_probe::SpawnRequest,
    pub(crate) source: tempfile::TempDir,
    pub(crate) workspace: tempfile::TempDir,
}

pub(crate) fn prepare(root: &Path, snapshot: &SourceSnapshot) -> Result<PreparedRuntime> {
    ensure!(
        root.is_absolute(),
        "managed environment root must be absolute"
    );
    snapshot.definition().validate()?;
    let spec = super::catalog::spec(root, &snapshot.definition().environment)?;
    ensure!(
        !matches!(spec.runtime, super::RuntimeKind::Shell),
        "persistent Shell projects are unsupported"
    );
    let info = super::catalog::info(root, spec)?;
    ensure!(info.ready, "environment is not ready; provision it first");
    let executable = launcher(&info)?;
    let roots = environment_roots(root, &info, &executable)?;
    stage(root, snapshot, info, executable, roots)
}

fn launcher(info: &super::EnvironmentInfo) -> Result<PathBuf> {
    let executable = match info.spec.runtime {
        super::RuntimeKind::Python => info.project.join(".venv/bin/python"),
        _ => info.executable.clone(),
    };
    let parent = executable
        .parent()
        .context("interpreter has no parent")?
        .canonicalize()?;
    let name = executable
        .file_name()
        .context("interpreter has no filename")?;
    let executable = parent.join(name);
    ensure!(executable.is_file(), "prepared interpreter is unavailable");
    Ok(executable)
}

fn environment_roots(
    root: &Path,
    info: &super::EnvironmentInfo,
    executable: &Path,
) -> Result<Vec<PathBuf>> {
    let projects = root.join("projects").canonicalize()?;
    ensure!(
        fs::symlink_metadata(projects.join(&info.spec.id))?
            .file_type()
            .is_symlink(),
        "persistent execution requires an immutable provisioned revision; provision this environment again"
    );
    ensure!(
        info.project.parent() == Some(projects.as_path()),
        "environment revision escapes managed projects"
    );
    let installs = root.join("mise-data/installs").canonicalize()?;
    let resolved = executable.canonicalize()?;
    let mut roots = vec![info.project.clone(), install_root(&installs, &resolved)?];
    let tools = info.project.join("tool-roots.json");
    if tools.try_exists()? {
        let bytes = fs::read(tools)?;
        ensure!(bytes.len() <= 64 * 1024, "tool roots exceed limit");
        let paths: Vec<PathBuf> = serde_json::from_slice(&bytes)?;
        ensure!(paths.len() <= 32, "too many tool roots");
        for path in paths {
            roots.push(install_root(&installs, &path.canonicalize()?)?);
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn install_root(installs: &Path, path: &Path) -> Result<PathBuf> {
    let relative = path
        .strip_prefix(installs)
        .context("interpreter or tool is outside managed installations")?;
    let mut components = relative.components();
    let tool = components.next().context("missing installed tool")?;
    let version = components.next().context("missing installed version")?;
    let root = installs.join(tool).join(version);
    ensure!(root.is_dir(), "managed install root is unavailable");
    Ok(root)
}

fn stage(
    root: &Path,
    snapshot: &SourceSnapshot,
    info: super::EnvironmentInfo,
    executable: PathBuf,
    mut roots: Vec<PathBuf>,
) -> Result<PreparedRuntime> {
    let sessions = root.join("sessions");
    fs::create_dir_all(&sessions)?;
    let sessions = sessions.canonicalize()?;
    let source = tempfile::Builder::new()
        .prefix("source-")
        .tempdir_in(&sessions)?;
    let workspace = tempfile::Builder::new()
        .prefix("scratch-")
        .tempdir_in(&sessions)?;
    write_source(source.path(), snapshot)?;
    let args = helper(source.path(), snapshot, &info)?;
    roots.push(source.path().to_path_buf());
    let mut request =
        mxc_probe::SpawnRequest::new(executable, roots, workspace.path().to_path_buf());
    request.args = args;
    Ok(PreparedRuntime {
        request,
        source,
        workspace,
    })
}

fn write_source(directory: &Path, snapshot: &SourceSnapshot) -> Result<()> {
    for (relative, bytes) in snapshot.files() {
        crate::project::validate_source_path(relative)?;
        let path = directory.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    }
    Ok(())
}

fn helper(
    directory: &Path,
    snapshot: &SourceSnapshot,
    info: &super::EnvironmentInfo,
) -> Result<Vec<String>> {
    use super::RuntimeKind;
    let (name, script, mut args) = match info.spec.runtime {
        RuntimeKind::Python => (
            ".subspace-worker.py",
            include_str!("../project_runtime/python.py"),
            vec!["-I".into(), "-u".into()],
        ),
        RuntimeKind::Node | RuntimeKind::Bun => (
            ".subspace-worker.mjs",
            include_str!("../project_runtime/javascript.mjs"),
            Vec::new(),
        ),
        RuntimeKind::Shell => bail!("persistent Shell projects are unsupported"),
    };
    use std::io::Write as _;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))?
        .write_all(script.as_bytes())?;
    #[cfg(unix)]
    if matches!(info.spec.runtime, RuntimeKind::Node | RuntimeKind::Bun)
        && info.project.join("node_modules").is_dir()
    {
        std::os::unix::fs::symlink(
            info.project.join("node_modules"),
            directory.join("node_modules"),
        )?;
    }
    args.push(directory.join(name).to_string_lossy().into_owned());
    args.push(
        directory
            .join(&snapshot.definition().entrypoint)
            .to_string_lossy()
            .into_owned(),
    );
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::prepare;
    use crate::project::{ProjectDefinition, ProjectStore};
    use std::collections::BTreeMap;
    use std::fs;

    #[cfg(unix)]
    #[test]
    fn project_runtime_plan_preserves_venv_identity_and_stages_only_declared_snapshot_files() {
        let directory = tempfile::tempdir().expect("fixture root");
        let root = directory.path().canonicalize().unwrap();
        let environment = root.join("environments");
        let revision = environment.join("projects/.python-data.revision-fixture");
        let install = environment.join("mise-data/installs/python/3.12.8");
        fs::create_dir_all(revision.join(".venv/bin")).unwrap();
        fs::create_dir_all(install.join("bin")).unwrap();
        // Planning-only fixture, never executable and never used as process proof.
        fs::write(install.join("bin/python"), "not an executable").unwrap();
        std::os::unix::fs::symlink(
            install.join("bin/python"),
            revision.join(".venv/bin/python"),
        )
        .unwrap();
        fs::write(
            revision.join("pyproject.toml"),
            "[project]\nname='fixture'\n",
        )
        .unwrap();
        fs::write(revision.join("uv.lock"), "version = 1\n").unwrap();
        fs::write(revision.join(".ready"), "ready\n").unwrap();
        std::os::unix::fs::symlink(&revision, environment.join("projects/python-data")).unwrap();
        let store = ProjectStore::new(root.join("boards")).unwrap();
        let mut definition = ProjectDefinition::new("fixture", "python-data", "main.py");
        definition.files.push("sibling.py".into());
        let files = BTreeMap::from([
            ("main.py".into(), b"import sibling\n".to_vec()),
            ("sibling.py".into(), b"VALUE = 27\n".to_vec()),
        ]);
        let loaded = store.create(&definition, files).unwrap();
        let snapshot = store.snapshot("fixture", &loaded.revision).unwrap();
        let plan = prepare(&environment, &snapshot).expect("staged persistent plan");
        assert_eq!(plan.request.executable, revision.join(".venv/bin/python"));
        assert!(plan.request.args.starts_with(&["-I".into(), "-u".into()]));
        assert!(plan.source.path().join("sibling.py").is_file());
        assert!(!plan.request.readonly_roots.contains(&environment));
        assert!(!plan.request.readonly_roots.contains(&root));
        assert!(plan.request.readonly_roots.contains(&install));
        assert!(plan.request.readonly_roots.contains(&revision));
        assert_ne!(plan.workspace.path(), snapshot.directory());
    }
}
