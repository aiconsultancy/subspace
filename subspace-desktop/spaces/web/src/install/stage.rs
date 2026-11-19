use std::path::{Path, PathBuf};
use subspace_framework::client_github::BoundRead;
use subspace_framework::core::config::web_boards::WebBoardSource;

use super::{
    ImportRequest, ManagedSource, PreparedImport, Snapshot, Summary, archive, files, github,
};
use crate::manifest;

pub(super) fn prepare(
    request: ImportRequest,
    store: &Path,
    bound: Option<&BoundRead>,
) -> Result<PreparedImport, String> {
    let owned = private_stage(store)?;
    let destination = owned.path().join("app");
    std::fs::create_dir(&destination).map_err(|_| "Cannot create app staging folder.")?;
    let (root, metadata) = populate(request, &destination, bound)?;
    let summary = validate(&root)?;
    readonly(&root)?;
    Ok(PreparedImport {
        snapshot: Snapshot {
            source: WebBoardSource::Folder(root),
            metadata,
            summary: Some(summary),
            owned: Some(owned),
        },
    })
}

fn private_stage(store: &Path) -> Result<tempfile::TempDir, String> {
    std::fs::create_dir_all(store).map_err(|_| "Cannot create private board store.")?;
    let metadata = std::fs::symlink_metadata(store).map_err(|_| "Cannot inspect board store.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Board store must be a private directory, not a link.".into());
    }
    tempfile::Builder::new()
        .prefix("board-")
        .tempdir_in(store)
        .map_err(|_| "Cannot create private board staging folder.".into())
}

fn populate(
    request: ImportRequest,
    destination: &Path,
    bound: Option<&BoundRead>,
) -> Result<(PathBuf, ManagedSource), String> {
    match request {
        ImportRequest::Folder { path } => folder(&path, destination),
        ImportRequest::Zip { path } => local_zip(&path, destination),
        request => github::populate(
            request,
            destination,
            bound.ok_or("Sign in to GitHub with an eligible organization account.")?,
        ),
    }
}

fn folder(path: &Path, destination: &Path) -> Result<(PathBuf, ManagedSource), String> {
    let original = path
        .canonicalize()
        .map_err(|_| "Cannot resolve selected folder.")?;
    let canonical_destination = destination
        .canonicalize()
        .map_err(|_| "Cannot resolve staging folder.")?;
    if canonical_destination.starts_with(&original) {
        return Err("Select an app folder outside the managed board store.".into());
    }
    files::copy(path, destination)?;
    let content = files::fingerprint(destination)?;
    Ok((
        destination.into(),
        ManagedSource::Folder { original, content },
    ))
}

fn local_zip(path: &Path, destination: &Path) -> Result<(PathBuf, ManagedSource), String> {
    let bytes = files::read_zip(path)?;
    archive::extract(&bytes, destination)?;
    let root = package_root(destination)?;
    let content = files::fingerprint(&root)?;
    let original = path
        .canonicalize()
        .map_err(|_| "Cannot resolve selected ZIP.")?;
    Ok((root, ManagedSource::Zip { original, content }))
}

pub(super) fn package_root(destination: &Path) -> Result<PathBuf, String> {
    if destination.join(manifest::MANIFEST).exists() || destination.join("index.html").exists() {
        return Ok(destination.into());
    }
    let mut entries = std::fs::read_dir(destination).map_err(|_| "Cannot read package root.")?;
    let first = entries
        .next()
        .ok_or("The ZIP is empty.")?
        .map_err(|_| "Cannot read package root.")?;
    if entries.next().is_some()
        || !first
            .file_type()
            .map_err(|_| "Cannot inspect package root.")?
            .is_dir()
    {
        return Err("ZIP must contain a board at its root or in one enclosing directory.".into());
    }
    Ok(first.path())
}

pub(super) fn validate(root: &Path) -> Result<Summary, String> {
    files::regular(&root.join("index.html")).map_err(
        |_| "The board root needs a regular, readable index.html. Select the built app folder.",
    )?;
    files::regular(&root.join(manifest::MANIFEST))
        .map_err(|_| "The board root needs a regular, readable subspace-board.json.")?;
    let manifest = manifest::read(root)?.ok_or("The board root needs subspace-board.json.")?;
    Ok(Summary {
        name: manifest.name,
        version: manifest.version,
        permissions: manifest
            .permissions
            .iter()
            .map(|permission| permission.name().into())
            .collect(),
        warnings: manifest.warnings,
    })
}

fn readonly(root: &Path) -> Result<(), String> {
    for path in files::listing(root)? {
        let path = root.join(path);
        let mut permissions = std::fs::metadata(&path)
            .map_err(|_| "Cannot inspect installed assets.")?
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(path, permissions)
            .map_err(|_| "Cannot protect installed assets.")?;
    }
    Ok(())
}
