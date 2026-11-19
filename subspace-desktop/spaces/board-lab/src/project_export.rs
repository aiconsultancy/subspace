//! Export an explicit source artifact list, never the local workbench tree.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};

use crate::project::{ProjectDefinition, limits, validate_source_path};

const PUBLIC_SOURCE_MODE: u32 = 0o644;
const FILE_READ_LIMIT: u64 = limits::FILE_BYTES as u64 + 1;

/// Create a new archive without overwriting an existing path.
///
/// `secret_values` is an explicit host-only transfer used to reject source
/// containing known local credentials; it must never be returned to agent tools.
pub fn export_project(
    project: &Path,
    files: &[String],
    output: &Path,
    secret_values: &[String],
) -> Result<()> {
    let artifacts = collect_artifacts(project, files, secret_values)?;
    write_archive(output, artifacts)
}

fn collect_artifacts(
    project: &Path,
    files: &[String],
    secrets: &[String],
) -> Result<Vec<(String, Vec<u8>)>> {
    validate_file_list(files)?;
    ensure!(
        fs::symlink_metadata(project)?.is_dir(),
        "Project must be an ordinary directory"
    );
    let root = project.canonicalize()?;
    let (artifacts, _) = files.iter().try_fold(
        (Vec::new(), 0),
        |(mut artifacts, total), name| -> Result<_> {
            let bytes = read_artifact(&root, name)?;
            validate_artifact_size(&bytes)?;
            reject_credentials(&bytes, secrets)?;
            let total = validate_source_budget(name, bytes.len(), total)?;
            artifacts.push((name.clone(), shareable_bytes(name, bytes)?));
            Ok((artifacts, total))
        },
    )?;
    validate_declared_files(&artifacts)?;
    Ok(artifacts)
}

fn validate_source_budget(name: &str, size: usize, total: usize) -> Result<usize> {
    let total = total + if name == "dashboard.json" { 0 } else { size };
    ensure!(
        total <= limits::SOURCE_BYTES,
        "Export source exceeds the project limit"
    );
    Ok(total)
}

fn validate_declared_files(artifacts: &[(String, Vec<u8>)]) -> Result<()> {
    let definition = artifacts
        .iter()
        .find(|(name, _)| name == "dashboard.json")
        .context("Missing definition")?;
    let definition = ProjectDefinition::parse(&definition.1)?;
    let declared: BTreeSet<_> = definition.files.iter().map(String::as_str).collect();
    let selected: BTreeSet<_> = artifacts
        .iter()
        .filter(|(name, _)| name != "dashboard.json")
        .map(|(name, _)| name.as_str())
        .collect();
    ensure!(
        declared == selected,
        "Export must include exactly the declared source files"
    );
    Ok(())
}

fn validate_file_list(files: &[String]) -> Result<()> {
    ensure!(
        files.len() <= limits::SOURCE_FILES + 1
            && files.iter().any(|name| name == "dashboard.json"),
        "Export requires a definition and at most 32 source files"
    );
    let mut names = BTreeSet::new();
    for name in files {
        ensure!(
            names.insert(name.to_ascii_lowercase()),
            "Duplicate export path"
        );
        if name != "dashboard.json" {
            validate_source_path(name)?;
        }
    }
    Ok(())
}

fn read_artifact(root: &Path, name: &str) -> Result<Vec<u8>> {
    let path = validate_artifact_path(root, name)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(FILE_READ_LIMIT)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn validate_artifact_path(root: &Path, name: &str) -> Result<std::path::PathBuf> {
    let mut path = root.to_owned();
    for segment in name.split('/') {
        path.push(segment);
        ensure!(
            !fs::symlink_metadata(&path)?.is_symlink(),
            "Linked export paths are not allowed"
        );
    }
    ensure!(
        fs::metadata(&path)?.is_file(),
        "Export artifact must be a file"
    );
    Ok(path)
}

fn validate_artifact_size(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= limits::FILE_BYTES,
        "Export artifact exceeds the file limit"
    );
    Ok(())
}

fn reject_credentials(bytes: &[u8], secrets: &[String]) -> Result<()> {
    ensure!(
        !secrets
            .iter()
            .filter(|secret| !secret.is_empty())
            .any(|secret| {
                bytes
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes())
            }),
        "Export blocked because an artifact contains a configured private value"
    );
    Ok(())
}

fn shareable_bytes(name: &str, bytes: Vec<u8>) -> Result<Vec<u8>> {
    if name != "dashboard.json" {
        return Ok(bytes);
    }
    let mut definition = ProjectDefinition::parse(&bytes)?;
    // Destination declarations are execution grants. A shared copy starts without them.
    definition.destinations.clear();
    Ok(serde_json::to_vec_pretty(&definition)?)
}

fn write_archive(output: &Path, artifacts: Vec<(String, Vec<u8>)>) -> Result<()> {
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut archive = zip::ZipWriter::new(temporary.as_file_mut());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(PUBLIC_SOURCE_MODE);
        for (name, bytes) in artifacts {
            archive.start_file(name, options)?;
            archive.write_all(&bytes)?;
        }
        archive.finish()?;
    }
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(output)
        .map_err(|_| anyhow::anyhow!("Cannot create export at the selected destination"))?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .context("Sync export directory")?;
    Ok(())
}

#[cfg(test)]
#[path = "project_export/tests.rs"]
mod tests;
