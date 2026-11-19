use sha2::{Digest, Sha256};
use std::{
    fs::{File, Metadata},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use super::{
    archive,
    paths::{self, Names},
};

pub(super) const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 10_000;
const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
const BUFFER_BYTES: usize = 32 * 1024;

#[derive(Default)]
pub(super) struct Budget(u64);
impl Budget {
    pub(super) fn copy(
        &mut self,
        input: &mut impl Read,
        output: &mut impl Write,
    ) -> Result<(), String> {
        let mut buffer = [0; BUFFER_BYTES];
        let mut size = 0u64;
        loop {
            let count = input
                .read(&mut buffer)
                .map_err(|_| "Cannot read package file.")?;
            if count == 0 {
                return Ok(());
            }
            size += count as u64;
            self.0 += count as u64;
            if size > MAX_FILE_BYTES {
                return Err("A package file exceeds 32 MiB.".into());
            }
            if self.0 > MAX_EXPANDED_BYTES {
                return Err("Package expansion exceeds 256 MiB.".into());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|_| "Cannot write package file.")?;
        }
    }
}

pub(super) fn regular(path: &Path) -> Result<File, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "Cannot read a required package file.")?;
    require_regular(&metadata, linked(&metadata))?;
    let file = File::open(path).map_err(|_| "Cannot open a package file.")?;
    let opened = file
        .metadata()
        .map_err(|_| "Cannot inspect package file.")?;
    require_unchanged(same_file(&metadata, &opened))?;
    Ok(file)
}

fn require_regular(metadata: &Metadata, linked: bool) -> Result<(), String> {
    if !metadata.file_type().is_file() || linked {
        return Err("Package files must be regular files, not links or special files.".into());
    }
    Ok(())
}

fn require_unchanged(same: bool) -> Result<(), String> {
    if !same {
        return Err("Package changed while reading. Try again.".into());
    }
    Ok(())
}

#[cfg(unix)]
fn linked(metadata: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() != 1
}
#[cfg(not(unix))]
fn linked(_: &Metadata) -> bool {
    false
}

#[cfg(unix)]
fn same_file(before: &Metadata, after: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.dev() == after.dev()
        && before.ino() == after.ino()
        && after.is_file()
        && after.nlink() == 1
}
#[cfg(not(unix))]
fn same_file(before: &Metadata, after: &Metadata) -> bool {
    after.is_file()
        && before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
}

pub(super) fn read_zip(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    regular(path)?
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read ZIP.")?;
    check_zip_size(bytes.len() as u64)?;
    Ok(bytes)
}

fn check_zip_size(size: u64) -> Result<(), String> {
    if size > MAX_ARCHIVE_BYTES {
        return Err("ZIP exceeds 64 MiB.".into());
    }
    Ok(())
}

pub(super) fn copy(source: &Path, destination: &Path) -> Result<(), String> {
    let root = folder_root(source)?;
    let entries = listing(&root)?;
    let mut budget = Budget::default();
    for relative in entries {
        let path = root.join(&relative);
        let canonical = path.canonicalize().map_err(|_| "Package file changed.")?;
        require_contained(&root, &canonical)?;
        archive::copy_entry(
            &mut regular(&path)?,
            &destination.join(relative),
            &mut budget,
        )?;
    }
    Ok(())
}

fn folder_root(source: &Path) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(source).map_err(|_| "Cannot read selected folder.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Select a regular app folder, not a link.".into());
    }
    source
        .canonicalize()
        .map_err(|_| "Cannot resolve selected folder.".into())
}

fn require_contained(root: &Path, path: &Path) -> Result<(), String> {
    if !path.starts_with(root) {
        return Err("Package file escapes the selected folder.".into());
    }
    Ok(())
}

pub(super) fn listing(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![PathBuf::new()];
    let mut files = Vec::new();
    let mut names = Names::default();
    let mut count = 0;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(root.join(&directory))
            .map_err(|_| "Cannot read package directory.")?
        {
            let entry = entry.map_err(|_| "Cannot read package entry.")?;
            let path = directory.join(entry.file_name());
            let text = paths::text(&path)?;
            paths::relative(&text)?;
            let kind = entry
                .file_type()
                .map_err(|_| "Cannot inspect package entry.")?;
            if !kind.is_dir() && !kind.is_file() {
                return Err("Package links and special files are not supported.".into());
            }
            names.add(&text, kind.is_dir())?;
            count += 1;
            if count > MAX_ENTRIES {
                return Err("Package contains more than 10,000 entries.".into());
            }
            if kind.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(super) fn fingerprint(root: &Path) -> Result<String, String> {
    let mut hash = Sha256::new();
    for path in listing(root)? {
        let name = paths::text(&path)?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        let mut file = regular(&root.join(&path))?;
        hash.update(
            file.metadata()
                .map_err(|_| "Cannot inspect package file.")?
                .len()
                .to_le_bytes(),
        );
        hash_file(&mut hash, &mut file)?;
    }
    Ok(hex::encode(hash.finalize()))
}

fn hash_file(hash: &mut Sha256, file: &mut File) -> Result<(), String> {
    let mut buffer = [0; BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "Cannot fingerprint package.")?;
        if count == 0 {
            return Ok(());
        }
        hash.update(&buffer[..count]);
    }
}
