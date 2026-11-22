//! Stored UI choices (REQ-08): the state file behind a GPUI global. Keys are
//! `<dashboard>.<name>`. A change
//! stays in memory until [`save`] writes the file; the app saves shortly
//! after changes and when it quits. Without [`init`] (in tests), choices last
//! for the run only.

use std::path::PathBuf;

use gpui_kit::{App, AppContext as _, Global};
use subspace_core::state::{self, State};

/// The most members a stored list keeps (`prefs.ts`, `MAX_STORED`).
pub const MAX_STORED: usize = 400;

#[derive(Clone, Default)]
pub struct Prefs {
    state: State,
    file: Option<PathBuf>,
    dirty: bool,
}

impl Global for Prefs {}

/// Loads the state file from the platform state directory.
pub fn init(cx: &mut App) {
    init_file(state::path(), cx);
}

/// Initializes preferences from this product's state file, without relocation.
pub fn init_file(file: Option<PathBuf>, cx: &mut App) {
    let state = file.as_deref().map(State::load).unwrap_or_default();
    cx.set_global(Prefs {
        state,
        file,
        dirty: false,
    });
    crate::icon_button::initialize(cx);
    crate::avatar::initialize_icons(cx);
}

pub fn get(cx: &App, key: &str) -> Option<String> {
    cx.try_global::<Prefs>()?.state.get(key).map(str::to_string)
}

/// True when a stored value is not a string and has not been replaced.
pub fn has_invalid_value(cx: &App, key: &str) -> bool {
    cx.try_global::<Prefs>()
        .is_some_and(|prefs| prefs.state.has_invalid_value(key))
}

pub fn set(cx: &mut App, key: &str, value: impl Into<String>) {
    let value = value.into();
    if !cx.has_global::<Prefs>() || get(cx, key).as_deref() == Some(value.as_str()) {
        return;
    }
    let prefs = cx.global_mut::<Prefs>();
    prefs.state.set(key, value);
    prefs.dirty = true;
}

/// Removes `key`, such as a value that moved to a file of its own.
pub fn remove(cx: &mut App, key: &str) {
    if get(cx, key).is_none() && !has_invalid_value(cx, key) {
        return;
    }
    let prefs = cx.global_mut::<Prefs>();
    prefs.state.remove(key);
    prefs.dirty = true;
}

/// The private directory that holds this product's state file, for data a
/// board keeps on disk, such as a web view's browsing data. `None` without
/// a state file (in tests).
pub fn state_dir(cx: &App) -> Option<PathBuf> {
    let file = cx.try_global::<Prefs>()?.file.as_deref()?;
    file.parent().map(std::path::Path::to_path_buf)
}

/// Reads cache file `name` beside the state file (see
/// [`subspace_core::state::sibling`]); `None` when there is none.
pub fn read_cache(name: &str) -> Option<String> {
    std::fs::read_to_string(state::sibling(name)?).ok()
}

/// Writes cache file `name` beside the state file, off the UI thread.
/// The atomic writer creates the replacement with owner-only (0600) Unix permissions.
pub fn write_cache(name: &str, text: String, cx: &App) {
    let Some(file) = state::sibling(name) else {
        return;
    };
    cx.background_spawn(async move {
        if let Err(error) = state::write_atomic(&file, &text) {
            eprintln!("subspace: could not save {}: {error}", file.display());
        }
    })
    .detach();
}

/// A stored JSON array of strings; anything else reads as empty.
pub fn get_list(cx: &App, key: &str) -> Vec<String> {
    get(cx, key)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn set_list(cx: &mut App, key: &str, values: &[String]) {
    let start = values.len().saturating_sub(MAX_STORED);
    let text = serde_json::to_string(&values[start..]).unwrap_or_default();
    set(cx, key, text);
}

/// Writes the state file when something changed since the last save.
pub fn save(cx: &mut App) -> std::io::Result<()> {
    let Some(prefs) = cx.try_global::<Prefs>() else {
        return Ok(());
    };
    let (Some(file), true) = (prefs.file.clone(), prefs.dirty) else {
        return Ok(());
    };
    prefs.state.save(&file)?;
    cx.global_mut::<Prefs>().dirty = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use gpui_kit::TestAppContext;

    use super::{get, has_invalid_value, init_file, remove, save, set};

    #[gpui_kit::test]
    fn invalid_entry_survives_other_saves_until_explicitly_removed(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("state.json");
        fs::write(&file, r#"{"layout":{"bad":true},"other":"old"}"#).unwrap();

        cx.update(|cx| {
            init_file(Some(file.clone()), cx);
            assert!(has_invalid_value(cx, "layout"));
            assert_eq!(get(cx, "other").as_deref(), Some("old"));
            set(cx, "other", "new");
            save(cx).unwrap();
        });
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(saved["layout"]["bad"], true);
        assert_eq!(saved["other"], "new");

        cx.update(|cx| {
            remove(cx, "layout");
            assert!(!has_invalid_value(cx, "layout"));
            save(cx).unwrap();
        });
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert!(saved.get("layout").is_none());
        assert_eq!(saved["other"], "new");
    }
}
