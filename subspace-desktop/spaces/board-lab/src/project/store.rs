use super::{LoadedProject, ProjectDefinition, ProjectRevision, limits};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// An owned immutable source revision. No private settings, cache, result or
/// undeclared file is ever included. Clone retains content, not a working path.
#[derive(Clone, Debug)]
pub struct SourceSnapshot {
    definition: ProjectDefinition,
    revision: ProjectRevision,
    files: BTreeMap<String, Vec<u8>>,
    directory: PathBuf,
}

impl SourceSnapshot {
    // qual:allow(dry, boilerplate) reason: "Immutable snapshots expose readonly metadata; public mutable fields could invalidate their content-addressed identity."
    pub fn definition(&self) -> &ProjectDefinition {
        &self.definition
    }
    // qual:allow(dry, boilerplate) reason: "A source revision cannot be mutable independently of the immutable snapshot it identifies."
    pub fn revision(&self) -> &ProjectRevision {
        &self.revision
    }
    // qual:allow(dry, boilerplate) reason: "Readonly source bytes prevent callers modifying a validated immutable snapshot in memory."
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    // qual:allow(dry, boilerplate) reason: "The immutable snapshot location must remain bound to its validated content and revision."
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

/// Stores `dashboard.json` and explicitly declared source under
/// `<workbench>/projects/<id>`. Snapshots are a separate sibling; opening never
/// executes or provisions anything. Writes serialize across store instances.
#[derive(Clone, Debug)]
pub struct ProjectStore {
    root: PathBuf,
}

impl ProjectStore {
    pub fn new(workbench: impl AsRef<Path>) -> Result<Self> {
        ensure!(
            workbench.as_ref().is_absolute(),
            "workbench root must be absolute"
        );
        fs::create_dir_all(workbench.as_ref())?;
        let root = workbench.as_ref().canonicalize()?;
        super::disk::directory(&root.join("projects"))?;
        super::disk::directory(&root.join("project-snapshots"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn project_directory(&self, id: &str) -> Result<PathBuf> {
        super::validation::token(id)?;
        super::disk::descendant(&self.root, &format!("projects/{id}"))
    }

    pub fn list(&self) -> Result<Vec<LoadedProject>> {
        let projects = super::disk::descendant(&self.root, "projects")?;
        let mut result = Vec::new();
        for entry in fs::read_dir(projects)? {
            let entry = entry?;
            let id = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("invalid project directory name"))?;
            if id.starts_with('.') {
                continue;
            }
            result.push(self.load(&id)?);
        }
        result.sort_by(|left, right| left.definition.id.cmp(&right.definition.id));
        Ok(result)
    }

    pub fn open(&self, id: &str) -> Result<LoadedProject> {
        self.load(id)
    }

    pub fn load(&self, id: &str) -> Result<LoadedProject> {
        let directory = self.project_directory(id)?;
        let loaded = load_directory(&directory)?;
        ensure!(
            loaded.definition.id == id,
            "project id does not match directory"
        );
        Ok(loaded)
    }

    pub fn create(
        &self,
        definition: &ProjectDefinition,
        files: BTreeMap<String, Vec<u8>>,
    ) -> Result<LoadedProject> {
        definition.validate()?;
        validate_files(definition, &files)?;
        let _lock = super::disk::lock(&self.root)?;
        let projects = super::disk::descendant(&self.root, "projects")?;
        let destination = projects.join(&definition.id);
        ensure!(
            !destination.try_exists()? && fs::symlink_metadata(&destination).is_err(),
            "project already exists"
        );
        let staging = tempfile::Builder::new()
            .prefix(".create-")
            .tempdir_in(&projects)?;
        write_project(staging.path(), definition, &files)?;
        fs::rename(staging.path(), destination)?;
        self.load(&definition.id)
    }

    pub fn save(
        &self,
        definition: &ProjectDefinition,
        expected: &ProjectRevision,
    ) -> Result<LoadedProject> {
        definition.validate()?;
        let _lock = super::disk::lock(&self.root)?;
        let current = self.load(&definition.id)?;
        ensure!(&current.revision == expected, "project revision conflict");
        let files = read_sources(&current.directory, definition)?;
        validate_files(definition, &files)?;
        let bytes = serde_json::to_vec_pretty(definition)?;
        super::disk::atomic_write(&current.directory.join("dashboard.json"), &bytes)?;
        self.load(&definition.id)
    }

    pub fn edit_file(
        &self,
        id: &str,
        path: &str,
        bytes: &[u8],
        expected: &ProjectRevision,
    ) -> Result<LoadedProject> {
        super::validate_source_path(path)?;
        let _lock = super::disk::lock(&self.root)?;
        let mut current = self.load(id)?;
        ensure!(&current.revision == expected, "project revision conflict");
        ensure!(
            current.definition.files.iter().any(|file| file == path),
            "source file is not declared"
        );
        current.files.insert(path.into(), bytes.to_vec());
        validate_files(&current.definition, &current.files)?;
        let destination = super::disk::descendant(&current.directory, path)?;
        super::disk::atomic_write(&destination, bytes)?;
        self.load(id)
    }

    pub fn snapshot(&self, id: &str, expected: &ProjectRevision) -> Result<SourceSnapshot> {
        let _lock = super::disk::lock(&self.root)?;
        let current = self.load(id)?;
        ensure!(&current.revision == expected, "project revision conflict");
        let base = super::disk::descendant(&self.root, "project-snapshots")?;
        super::disk::directory(&base.join(id))?;
        let destination = base.join(id).join(&current.revision.source);
        persist_snapshot(&current, &destination)?;
        self.open_snapshot(id, &current.revision.source)
    }

    pub fn open_snapshot(&self, id: &str, source_revision: &str) -> Result<SourceSnapshot> {
        super::validation::token(id)?;
        validate_revision(source_revision)?;
        let path = super::disk::descendant(
            &self.root,
            &format!("project-snapshots/{id}/{source_revision}"),
        )?;
        let loaded = load_directory(&path)?;
        ensure!(
            loaded.definition.id == id && loaded.revision.source == source_revision,
            "snapshot content does not match immutable revision"
        );
        Ok(SourceSnapshot {
            definition: loaded.definition,
            revision: loaded.revision,
            files: loaded.files,
            directory: loaded.directory,
        })
    }
}

fn load_directory(directory: &Path) -> Result<LoadedProject> {
    let path = super::disk::descendant(directory, "dashboard.json")?;
    let bytes = super::disk::read(&path, limits::DEFINITION_BYTES)?;
    let definition = ProjectDefinition::parse(&bytes)?;
    let files = read_sources(directory, &definition)?;
    validate_files(&definition, &files)?;
    let revision = revision(&bytes, &files);
    Ok(LoadedProject {
        definition,
        revision,
        files,
        directory: directory.to_path_buf(),
    })
}

fn read_sources(
    directory: &Path,
    definition: &ProjectDefinition,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut total = 0;
    for relative in &definition.files {
        let path = super::disk::descendant(directory, relative)?;
        let bytes = super::disk::read(&path, limits::FILE_BYTES)?;
        total += bytes.len();
        ensure!(total <= limits::SOURCE_BYTES, "source exceeds 2 MiB total");
        files.insert(relative.clone(), bytes);
    }
    Ok(files)
}

fn validate_files(definition: &ProjectDefinition, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let declared: BTreeSet<_> = definition.files.iter().collect();
    let provided: BTreeSet<_> = files.keys().collect();
    ensure!(
        declared == provided,
        "source files must exactly match the definition allowlist"
    );
    ensure!(
        files
            .values()
            .all(|bytes| bytes.len() <= limits::FILE_BYTES),
        "source file exceeds 256 KiB"
    );
    ensure!(
        files.values().map(Vec::len).sum::<usize>() <= limits::SOURCE_BYTES,
        "source exceeds 2 MiB total"
    );
    Ok(())
}

fn revision(definition: &[u8], files: &BTreeMap<String, Vec<u8>>) -> ProjectRevision {
    let definition_hash = hex::encode(Sha256::digest(definition));
    let mut hash = Sha256::new();
    hash.update(b"subspace-project-source-v1\0");
    hash.update(&definition_hash);
    for (path, bytes) in files {
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    ProjectRevision {
        definition: definition_hash,
        source: hex::encode(hash.finalize()),
    }
}

fn write_project(
    directory: &Path,
    definition: &ProjectDefinition,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    super::disk::write_tree(directory, files)?;
    super::disk::atomic_write(
        &directory.join("dashboard.json"),
        &serde_json::to_vec_pretty(definition)?,
    )
}

fn persist_snapshot(current: &LoadedProject, destination: &Path) -> Result<()> {
    if destination.try_exists()? {
        let existing = load_directory(destination)?;
        ensure!(
            existing.revision == current.revision,
            "immutable snapshot was changed"
        );
        return Ok(());
    }
    let base = destination.parent().context("snapshot has no parent")?;
    let staging = tempfile::Builder::new()
        .prefix(".snapshot-")
        .tempdir_in(base)?;
    super::disk::write_tree(staging.path(), &current.files)?;
    let bytes = super::disk::read(
        &current.directory.join("dashboard.json"),
        limits::DEFINITION_BYTES,
    )?;
    super::disk::atomic_write(&staging.path().join("dashboard.json"), &bytes)?;
    let staged = load_directory(staging.path())?;
    ensure!(
        staged.revision == current.revision,
        "source changed during snapshot creation"
    );
    fs::rename(staging.path(), destination)?;
    Ok(())
}

fn validate_revision(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "invalid source revision"
    );
    Ok(())
}
