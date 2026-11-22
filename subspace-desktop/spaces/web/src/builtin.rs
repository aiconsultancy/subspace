//! Web Spaces an app ships inside its binary, such as Fleet Console. At
//! start the app writes the files into a folder of its own and makes sure
//! the config file has a `webBoards` entry that shows them, so the Space is
//! in the tab strip without an install. From then on it is an ordinary
//! folder web board: its settings, secrets and grants live in its entry.
//!
//! The entry is the built-in one while it carries `"builtIn": true` and no
//! `managedSource`. An entry the user installed under the same ID, from a
//! folder, a ZIP, GitHub or a URL, is theirs and is left alone, files and
//! all. An entry with neither `url` nor `path`, as an installer writes to set
//! the Space's address and token before the app first starts, is adopted.

mod config_file;
mod entry;
mod files;

use std::path::Path;

use serde_json::{Map, Value};

use config_file::{parse_config, read_text, save_unless_changed, write_target};
use entry::{Package, claimed, planned_entry};
use files::sync_files;

/// The board ID of the built-in Fleet Console. Built in under this ID, it
/// is the one web Space whose bridge answers `fleets.get` (see
/// `bridge::fleets`).
pub const FLEET_CONSOLE_ID: &str = "fleet-console";

/// The entry key that marks a built-in web Space.
pub const BUILT_IN_KEY: &str = "builtIn";

/// How many times start-up reads, plans and saves the config file when
/// another writer changes it in between.
const SAVE_ATTEMPTS: usize = 3;

/// One web Space compiled into the app: its board ID, its title in the tab
/// strip and its package files, as `(relative path, bytes)` with `/`
/// separators.
pub struct BuiltIn {
    pub id: &'static str,
    pub title: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

/// Adds or updates `app`'s entry in `config_file`, writing its files into
/// `store/<id>` when they differ from what is there. Neither the files nor
/// the config file is touched when the entry under `app`'s ID is the user's.
/// Returns whether the config file changed. Call before the app reads its
/// config.
pub fn install(app: &BuiltIn, config_file: &Path, store: &Path) -> Result<bool, String> {
    let config_file = write_target(config_file)?;
    let saved = retried(SAVE_ATTEMPTS, || attempt(app, &config_file, store))?;
    saved.ok_or_else(|| {
        format!(
            "{} kept changing while the built-in web Spaces were added.",
            config_file.display()
        )
    })
}

/// Runs `step` up to `attempts` times, until it returns an outcome.
fn retried<T>(
    attempts: usize,
    mut step: impl FnMut() -> Result<Option<T>, String>,
) -> Result<Option<T>, String> {
    for _ in 0..attempts {
        if let Some(outcome) = step()? {
            return Ok(Some(outcome));
        }
    }
    Ok(None)
}

/// Reads, plans and saves the config file once: whether it saved a change,
/// or `None` when another writer changed the file in the meantime.
fn attempt(app: &BuiltIn, file: &Path, store: &Path) -> Result<Option<bool>, String> {
    let original = read_text(file)?;
    let planned = planned_config(app, file, original.as_deref(), store)?;
    planned.map_or(Ok(Some(false)), |text| {
        save_unless_changed(file, original.as_deref(), &text).map(|saved| saved.then_some(true))
    })
}

/// Whether raw config entry `entry` is a built-in web Space's.
pub(crate) fn is_built_in(entry: &Map<String, Value>) -> bool {
    entry.get(BUILT_IN_KEY) == Some(&Value::Bool(true))
        && !entry.contains_key("managedSource")
        && !entry.contains_key("url")
}

/// The config file's text with `app`'s entry added or updated, or `None`
/// when it needs no change or the entry is the user's. `original` is the
/// file's text, `None` when it is missing.
fn planned_config(
    app: &BuiltIn,
    file: &Path,
    original: Option<&str>,
    store: &Path,
) -> Result<Option<String>, String> {
    let mut config = parse_config(file, original.unwrap_or_default())?;
    let boards = web_boards(&mut config)?;
    let entry = claimed(boards.get(app.id))
        .map(|(existing, built_in)| entry_for(app, store, existing, built_in))
        .transpose()?;
    let changed = replaced(boards, app.id, entry);
    changed.then(|| pretty(config)).transpose()
}

/// The config's `webBoards` object, added when it has none.
fn web_boards(config: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    config
        .entry("webBoards")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "webBoards in the config file is not an object.".to_owned())
}

/// `app`'s files written into `store`, then the entry to save for them,
/// planned from `existing` (see [`planned_entry`]).
fn entry_for(
    app: &BuiltIn,
    store: &Path,
    existing: Map<String, Value>,
    built_in: bool,
) -> Result<Map<String, Value>, String> {
    let root = sync_files(app, store)?;
    let package = Package::read(&root, app.title)?;
    Ok(planned_entry(existing, built_in, &package))
}

/// Puts `entry` under `id` in `boards` when there is one and it differs from
/// the saved one. Returns whether it did.
fn replaced(boards: &mut Map<String, Value>, id: &str, entry: Option<Map<String, Value>>) -> bool {
    let Some(entry) = entry.map(Value::Object) else {
        return false;
    };
    if boards.get(id) == Some(&entry) {
        return false;
    }
    boards.insert(id.to_owned(), entry);
    true
}

/// The config file's text for `config`.
fn pretty(config: Map<String, Value>) -> Result<String, String> {
    serde_json::to_string_pretty(&Value::Object(config))
        .map(|text| text + "\n")
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
