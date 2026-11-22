//! Writing a built-in Space's package files into its folder in the store.

use std::path::{Path, PathBuf};

use super::BuiltIn;

/// `store/<id>` holding exactly `app`'s files. Unchanged files are left as
/// they are; otherwise the folder is rebuilt beside it and swapped in. A
/// link in its place is moved aside, never followed.
pub(super) fn sync_files(app: &BuiltIn, store: &Path) -> Result<PathBuf, String> {
    crate::install::require_store(store)?;
    let root = store.join(app.id);
    if matches(app.files, &root) {
        return Ok(root);
    }
    let staged = tempfile::Builder::new()
        .prefix(".built-in-")
        .tempdir_in(store)
        .map_err(|_| "Cannot create a staging folder.")?;
    let fresh = staged.path().join("app");
    write_files(app.files, &fresh)?;
    swap_in(&fresh, &root, &staged.path().join("previous"))?;
    Ok(root)
}

/// Writes `files` under `folder`.
fn write_files(files: &[(&str, &[u8])], folder: &Path) -> Result<(), String> {
    for (relative, bytes) in files {
        let file = joined(folder, relative).ok_or("A built-in file has an unsafe name.")?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "Cannot create a built-in folder.")?;
        }
        std::fs::write(&file, bytes).map_err(|_| "Cannot write a built-in file.")?;
    }
    Ok(())
}

/// Moves `fresh` to `root`, first moving whatever is at `root` to `aside`.
/// When `fresh` cannot be moved, what was at `root` goes back.
pub(super) fn swap_in(fresh: &Path, root: &Path, aside: &Path) -> Result<(), String> {
    let had_previous = std::fs::symlink_metadata(root).is_ok();
    if had_previous {
        std::fs::rename(root, aside)
            .map_err(|_| "Cannot move the previous built-in files aside.")?;
    }
    if std::fs::rename(fresh, root).is_err() {
        if had_previous {
            let _ = std::fs::rename(aside, root);
        }
        return Err("Cannot put the built-in files in place.".into());
    }
    Ok(())
}

/// `root/relative`, refusing empty, `.` and `..` segments.
fn joined(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = root.to_owned();
    for segment in relative.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\') {
            return None;
        }
        path.push(segment);
    }
    Some(path)
}

/// Whether `root` is a folder, not a link, holding exactly `files`, byte
/// for byte.
fn matches(files: &[(&str, &[u8])], root: &Path) -> bool {
    let is_folder = std::fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_dir());
    let mut found = Vec::new();
    if !is_folder || !list(root, "", &mut found) || found.len() != files.len() {
        return false;
    }
    files.iter().all(|(relative, bytes)| {
        joined(root, relative).is_some_and(|file| std::fs::read(file).is_ok_and(|on| on == *bytes))
    })
}

/// Collects the relative paths of the regular files under `folder`. False
/// when it cannot be read or holds anything else, such as a link.
fn list(folder: &Path, prefix: &str, found: &mut Vec<String>) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = format!("{prefix}{name}");
        if kind.is_dir() {
            if !list(&entry.path(), &format!("{relative}/"), found) {
                return false;
            }
        } else if kind.is_file() {
            found.push(relative);
        } else {
            return false;
        }
    }
    true
}
