//! Web Spaces an app ships inside its binary, such as Fleet Console. At
//! start the app writes the files into a folder of its own and makes sure
//! the config file has a `webBoards` entry that shows them, so the Space is
//! in the tab strip without an install. From then on it is an ordinary
//! folder web board: its settings, secrets and grants live in its entry.
//!
//! The entry is the built-in one while it carries `"builtIn": true` and no
//! `managedSource`. An entry the user installed under the same ID, from a
//! folder, a ZIP, GitHub or a URL, is theirs and is left alone. An entry with
//! neither `url` nor `path`, as an installer writes to set the Space's
//! address and token before the app first starts, is adopted.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::manifest;

/// The entry key that marks a built-in web Space.
pub const BUILT_IN_KEY: &str = "builtIn";

/// One web Space compiled into the app: its board ID, its title in the tab
/// strip and its package files, as `(relative path, bytes)` with `/`
/// separators.
pub struct BuiltIn {
    pub id: &'static str,
    pub title: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

/// Writes `app`'s files into `store/<id>` when they differ from what is
/// there, then adds or updates its entry in `config_file`. Returns whether
/// the config file changed. Call before the app reads its config.
pub fn install(app: &BuiltIn, config_file: &Path, store: &Path) -> Result<bool, String> {
    let root = sync_files(app, store)?;
    let grants = grant_keys(&root)?;
    let mut config = read_config(config_file)?;
    let boards = config
        .entry("webBoards")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or("webBoards in the config file is not an object.")?;
    let existing = boards.get(app.id).and_then(Value::as_object).cloned();
    if boards.get(app.id).is_some() && existing.is_none() {
        return Ok(false);
    }
    let Some(entry) = planned_entry(existing, &root, app.title, &grants) else {
        return Ok(false);
    };
    boards.insert(app.id.to_owned(), Value::Object(entry));
    let text =
        serde_json::to_string_pretty(&Value::Object(config)).map_err(|error| error.to_string())?;
    subspace_framework::core::state::write_atomic(config_file, &(text + "\n"))
        .map_err(|error| format!("Cannot save {}: {error}", config_file.display()))?;
    Ok(true)
}

/// The entry to save for a built-in Space at `root`, or `None` to leave the
/// saved one as it is. A new or adopted entry is granted every permission
/// in `grants`: the Space is the app's own code.
fn planned_entry(
    existing: Option<Map<String, Value>>,
    root: &Path,
    title: &str,
    grants: &[String],
) -> Option<Map<String, Value>> {
    let path = Value::String(root.to_string_lossy().into_owned());
    let mut entry = match existing {
        None => Map::new(),
        Some(entry) if entry.contains_key("managedSource") || entry.contains_key("url") => {
            return None;
        }
        Some(mut entry) if entry.get(BUILT_IN_KEY) == Some(&Value::Bool(true)) => {
            if entry.get("path") == Some(&path) {
                return None;
            }
            entry.insert("path".into(), path);
            return Some(entry);
        }
        Some(entry) if entry.contains_key("path") => return None,
        Some(entry) => entry,
    };
    entry
        .entry("title")
        .or_insert_with(|| Value::String(title.to_owned()));
    entry.insert("path".into(), path);
    entry.insert(BUILT_IN_KEY.into(), Value::Bool(true));
    let permissions = entry
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(permissions) = permissions.as_object_mut() {
        for key in grants {
            permissions.entry(key.clone()).or_insert(Value::Bool(true));
        }
    }
    Some(entry)
}

/// The grant keys of every permission the manifest at `root` asks for.
fn grant_keys(root: &Path) -> Result<Vec<String>, String> {
    let manifest = manifest::read(root)?.ok_or("The built-in web Space has no manifest.")?;
    Ok(manifest
        .permissions
        .iter()
        .map(|permission| manifest.grant_key(root, *permission))
        .collect())
}

/// The config file's object; an empty one when the file is missing or blank.
fn read_config(file: &Path) -> Result<Map<String, Value>, String> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Cannot read {}: {error}", file.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(config)) => Ok(config),
        _ => Err(format!(
            "{} is not a JSON object; the built-in web Spaces were not added.",
            file.display()
        )),
    }
}

/// `store/<id>` holding exactly `app`'s files. Unchanged files are left as
/// they are; otherwise the folder is rebuilt beside it and swapped in.
fn sync_files(app: &BuiltIn, store: &Path) -> Result<PathBuf, String> {
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
    for (relative, bytes) in app.files {
        let file = joined(&fresh, relative).ok_or("A built-in file has an unsafe name.")?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "Cannot create a built-in folder.")?;
        }
        std::fs::write(&file, bytes).map_err(|_| "Cannot write a built-in file.")?;
    }
    if root.exists() {
        std::fs::rename(&root, staged.path().join("previous"))
            .map_err(|_| "Cannot move the previous built-in files aside.")?;
    }
    std::fs::rename(&fresh, &root).map_err(|_| "Cannot put the built-in files in place.")?;
    Ok(root)
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

/// Whether `root` holds exactly `files`, byte for byte.
fn matches(files: &[(&str, &[u8])], root: &Path) -> bool {
    let mut found = Vec::new();
    if !list(root, "", &mut found) || found.len() != files.len() {
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

#[cfg(test)]
mod tests;
