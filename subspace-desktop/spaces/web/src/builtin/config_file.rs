//! Reading and saving the config file at start, beside other writers such
//! as Settings.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The file to save the config to: the target of `file` when it is a link,
/// so the link stays in place.
pub(super) fn write_target(file: &Path) -> Result<PathBuf, String> {
    let is_link = std::fs::symlink_metadata(file).is_ok_and(|meta| meta.file_type().is_symlink());
    if !is_link {
        return Ok(file.to_owned());
    }
    std::fs::canonicalize(file)
        .map_err(|error| format!("Cannot follow {}: {error}", file.display()))
}

/// The config file's text, or `None` when it is missing.
pub(super) fn read_text(file: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Cannot read {}: {error}", file.display())),
    }
}

/// The config file's object from its text; an empty one when it is blank.
pub(super) fn parse_config(file: &Path, text: &str) -> Result<Map<String, Value>, String> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(config)) => Ok(config),
        _ => Err(format!(
            "{} is not a JSON object; the built-in web Spaces were not added.",
            file.display()
        )),
    }
}

/// Saves `text` to `file` unless the file no longer holds `original`, as
/// another writer such as Settings changed it since it was read. Returns
/// whether it saved. Nothing locks the file, so a write in the moment
/// between the check and the save can still be lost.
pub(super) fn save_unless_changed(
    file: &Path,
    original: Option<&str>,
    text: &str,
) -> Result<bool, String> {
    if read_text(file)?.as_deref() != original {
        return Ok(false);
    }
    subspace_framework::core::state::write_atomic(file, text)
        .map_err(|error| format!("Cannot save {}: {error}", file.display()))?;
    Ok(true)
}
