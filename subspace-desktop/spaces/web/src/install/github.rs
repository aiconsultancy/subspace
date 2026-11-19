use std::path::{Path, PathBuf};
use subspace_framework::client_github::BoundRead;

use super::{ImportRequest, ManagedSource, archive, files, paths, stage};

pub(super) fn populate(
    request: ImportRequest,
    destination: &Path,
    bound: &BoundRead,
) -> Result<(PathBuf, ManagedSource), String> {
    match request {
        ImportRequest::Repository {
            repository,
            reference,
            board_path,
        } => {
            let resolved = bound
                .resolve_repository(&repository, &reference)
                .map_err(safe)?;
            repository_root(bound, resolved, &board_path, destination)
        }
        ImportRequest::RepositorySnapshot {
            repository,
            reference,
            board_path,
            commit,
        } => {
            let mut resolved = bound
                .resolve_repository(
                    &repository,
                    &subspace_framework::client_github::RepositoryRef::Commit(commit),
                )
                .map_err(safe)?;
            resolved.reference = reference;
            repository_root(bound, resolved, &board_path, destination)
        }
        ImportRequest::Release {
            repository,
            release_id,
            asset_name,
            include_prereleases,
        } => release_root(
            bound,
            destination,
            ReleaseChoice {
                repository,
                release_id,
                asset_name,
                include_prereleases,
            },
        ),
        _ => Err("Select a supported import source.".into()),
    }
}

fn repository_root(
    bound: &BoundRead,
    resolved: subspace_framework::client_github::ResolvedRepository,
    board_path: &str,
    destination: &Path,
) -> Result<(PathBuf, ManagedSource), String> {
    let relative = if board_path.is_empty() {
        PathBuf::new()
    } else {
        paths::relative(board_path)?
    };
    let bytes = bound
        .download_repository(&resolved.repository, &resolved.commit)
        .map_err(safe)?;
    let scratch = tempfile::Builder::new()
        .prefix("download-")
        .tempdir_in(destination.parent().ok_or("Invalid staging location.")?)
        .map_err(|_| "Cannot stage repository archive.")?;
    archive::extract(&bytes, scratch.path())?;
    let root = stage::package_root(scratch.path())?.join(relative);
    validate_repository_board(&root, board_path)?;
    files::copy(&root, destination)?;
    let content = files::fingerprint(destination)?;
    let metadata = ManagedSource::Repository {
        repository: resolved.repository,
        reference: resolved.reference,
        board_path: board_path.into(),
        commit: resolved.commit,
        content,
    };
    Ok((destination.into(), metadata))
}

fn validate_repository_board(root: &Path, board_path: &str) -> Result<(), String> {
    repository_directory(root)
        .and_then(|()| stage::validate(root).map(|_| ()))
        .map_err(|error| format!("GitHub board directory \"{board_path}\": {error}"))
}

fn repository_directory(root: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|_| "The directory was not found in the repository archive.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Select a regular app directory, not a link.".into());
    }
    Ok(())
}

struct ReleaseChoice {
    repository: String,
    release_id: u64,
    asset_name: String,
    include_prereleases: bool,
}

fn release_root(
    bound: &BoundRead,
    destination: &Path,
    choice: ReleaseChoice,
) -> Result<(PathBuf, ManagedSource), String> {
    let release = bound
        .package_release(&choice.repository, choice.release_id)
        .map_err(safe)?;
    let bytes = bound
        .download_release(
            &choice.repository,
            choice.release_id,
            &choice.asset_name,
            choice.include_prereleases,
        )
        .map_err(safe)?;
    archive::extract(&bytes, destination)?;
    let root = stage::package_root(destination)?;
    let content = files::fingerprint(&root)?;
    let metadata = ManagedSource::Release {
        repository: choice.repository,
        release_id: release.id,
        tag: release.tag_name,
        asset_name: choice.asset_name,
        include_prereleases: choice.include_prereleases,
        content,
    };
    Ok((root, metadata))
}

fn safe(error: subspace_framework::core::http::Error) -> String {
    error.to_string()
}
