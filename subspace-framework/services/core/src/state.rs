//! The state file (REQ-08): UI choices kept between runs. Keys are
//! namespaced `<dashboard>.<name>`, such as `board.layout`. Values are
//! strings; structured values are JSON text.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use serde_json::value::RawValue;
use subspace_product::Channel;

/// `<directory>/state.json` in the platform state directory:
/// `$XDG_STATE_HOME` on Linux, the local data directory elsewhere
/// (Application Support on macOS, `%LOCALAPPDATA%` on Windows).
/// [`directory_name`] gives the directory.
pub fn path() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|dir| dir.join(directory_name()).join("state.json"))
}

/// The Subspace data directory name of this build's channel: `subspace` on
/// stable, `subspace-<type>` for a pre-release type. Config, state and
/// caches that are not product-specific use it.
pub fn directory_name() -> String {
    directory_name_for(Channel::current())
}

fn directory_name_for(channel: Channel) -> String {
    channel.name("subspace")
}

/// `name` beside the state file, such as `github-cache.json`. Large
/// caches live in files of their own, so `state.json` stays small.
pub fn sibling(name: &str) -> Option<PathBuf> {
    path().and_then(|state| Some(state.parent()?.join(name)))
}

/// Writes `text` to `file` atomically: a temporary file in the same
/// directory, then a rename over the old file.
pub fn write_atomic(file: &Path, text: &str) -> io::Result<()> {
    let dir = file
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the file has no directory"))?;
    fs::create_dir_all(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    crate::private_file::set_owner_only(temp.path())?;
    temp.write_all(text.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(file).map_err(|error| error.error)?;
    Ok(())
}

/// True for bytes that hold only JSON whitespace (RFC 8259: space, tab,
/// carriage return, line feed). Form feed and vertical tab are not JSON
/// whitespace, so a file that holds only those is not blank.
fn is_blank(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

/// The exact JSON text of a value that is not a string, so a save writes
/// back every digit and is not limited to `f64` or `u64`/`i64`.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
struct RawJson(Box<RawValue>);

impl PartialEq for RawJson {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

impl Eq for RawJson {}

/// One value of the document that [`State::save`] writes.
#[derive(Serialize)]
#[serde(untagged)]
enum Entry<'a> {
    Text(&'a str),
    Raw(&'a RawJson),
}

/// Whether [`State::load`] found a file it could not use, until a save
/// succeeds. Interior mutability lets `save(&self)` clear it. It is not part
/// of a state's equality.
#[derive(Debug, Default)]
struct LoadFailed(AtomicBool);

impl LoadFailed {
    fn new(failed: bool) -> Self {
        Self(AtomicBool::new(failed))
    }

    fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn clear(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

impl Clone for LoadFailed {
    fn clone(&self) -> Self {
        Self::new(self.is_set())
    }
}

impl PartialEq for LoadFailed {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for LoadFailed {}

/// The state in the JSON object `bytes`, or `None` when they are not one.
/// A string value is a usable value; every other value is kept as raw JSON.
fn parse_state(bytes: &[u8]) -> Option<State> {
    let fields: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(bytes).ok()?;
    let mut state = State::default();
    for (key, raw) in fields {
        if let Ok(text) = serde_json::from_str::<String>(raw.get()) {
            state.values.insert(key, text);
        } else {
            state.invalid.insert(key, RawJson(raw));
        }
    }
    Some(state)
}

/// The state of the file. A missing or blank file is a normal first run. A
/// read error other than "not found", or content that is not a JSON object,
/// gives an empty state with the failure recorded.
fn read_state(file: &Path) -> State {
    let unusable = |failed: bool| State {
        load_failed: LoadFailed::new(failed),
        ..State::default()
    };
    match fs::read(file) {
        Ok(bytes) => parse_state(&bytes).unwrap_or_else(|| unusable(!is_blank(&bytes))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => State::default(),
        Err(_) => unusable(true),
    }
}

/// Copies the current state file to `<name>.invalid-<unix seconds>-<n>`
/// beside it, so a save cannot destroy it, when it is not a JSON object or,
/// given `unless_equal` (the state about to be saved), when it holds
/// anything else. A missing or blank file needs no copy. Any other failure to
/// read the file, or to write the copy, is an error and the caller must not
/// replace the file.
fn keep_unreadable_file(file: &Path, unless_equal: Option<&State>) -> io::Result<()> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    keep_unreadable_file_stamped(file, seconds, unless_equal)
}

fn keep_unreadable_file_stamped(
    file: &Path,
    stamp: u64,
    unless_equal: Option<&State>,
) -> io::Result<()> {
    let bytes = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let nothing_to_lose = parse_state(&bytes)
        .is_some_and(|current| unless_equal.is_none_or(|state| *state == current));
    if is_blank(&bytes) || nothing_to_lose {
        return Ok(());
    }
    let dir = file
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the file has no directory"))?;
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    // Written and synced under a private temporary name, then linked to the
    // first free `<name>.invalid-<stamp>-<n>`: an earlier copy is never replaced.
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    crate::private_file::set_owner_only(temp.path())?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    for attempt in 0_u32.. {
        let copy = dir.join(format!("{name}.invalid-{stamp}-{attempt}"));
        match temp.persist_noclobber(&copy) {
            Ok(_) => return Ok(()),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                temp = error.file;
            }
            Err(error) => return Err(error.error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free name for the state file copy",
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    values: BTreeMap<String, String>,
    /// File entries whose value is not a string. They are not readable
    /// through [`State::get`], but [`State::save`] writes them back
    /// with their exact JSON text until [`State::set`] or [`State::remove`]
    /// replaces them.
    invalid: BTreeMap<String, RawJson>,
    /// [`State::load`] found a file it could not use. Until a save succeeds,
    /// [`State::save`] keeps a copy of any different file it finds on disk.
    /// Ignored by equality.
    load_failed: LoadFailed,
}

impl State {
    /// Reads the state file. A missing or unreadable file gives the defaults.
    /// An entry whose value is not a string is kept apart (see
    /// [`State::has_invalid_value`]) and does not affect the other entries.
    /// A file that cannot be used gives the defaults: a read error other than
    /// "not found", or content that is not a JSON object. A missing or blank
    /// file is a normal first run. After an unusable file, the next
    /// [`State::save`] first copies whatever file it finds, unless it already
    /// holds exactly the document to save, to
    /// `<name>.invalid-<unix seconds>-<n>`. That covers a file that was
    /// malformed, and one that was only unreadable or half-written at load
    /// time and is valid by the time of the save. [`State::save`] fails,
    /// without replacing the file, if it cannot read the file or write the
    /// copy. Only space, tab, carriage return and line feed count as blank.
    /// A non-string value keeps its exact JSON text, such as a number with
    /// more digits than `u64` or `f64` hold.
    pub fn load(file: &Path) -> Self {
        read_state(file)
    }

    /// The string value of `key`. `None` for a missing key and for a key
    /// whose stored value is not a string: use [`State::has_invalid_value`]
    /// to tell them apart before calling [`State::set`].
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// True when the file holds a non-string value for `key` that has not
    /// been replaced or removed. A caller that must not overwrite unreadable
    /// data skips [`State::set`] while this is true; calling [`State::set`] or
    /// [`State::remove`] is the explicit reset.
    pub fn has_invalid_value(&self, key: &str) -> bool {
        self.invalid.contains_key(key)
    }

    /// Stores `value`, replacing any value, including an invalid one.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        self.invalid.remove(&key);
        self.values.insert(key, value.into());
    }

    /// Deletes `key`, including an invalid value.
    pub fn remove(&mut self, key: &str) {
        self.invalid.remove(key);
        self.values.remove(key);
    }

    fn document(&self) -> BTreeMap<&str, Entry<'_>> {
        let kept = self
            .invalid
            .iter()
            .map(|(key, raw)| (key.as_str(), Entry::Raw(raw)));
        let strings = self
            .values
            .iter()
            .map(|(key, text)| (key.as_str(), Entry::Text(text.as_str())));
        kept.chain(strings).collect()
    }

    /// Writes the file atomically: a temporary file in the same directory,
    /// then a rename over the old file. Invalid entries read by
    /// [`State::load`] are written back with their exact JSON text. A current
    /// file that is not a JSON object, or that differs from this state after
    /// a failed load, is copied aside first (see [`State::load`]). The first
    /// successful save after a failed load ends the check, so later saves
    /// make no more copies.
    pub fn save(&self, file: &Path) -> io::Result<()> {
        let dir = file.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the state file has no directory",
            )
        })?;
        fs::create_dir_all(dir)?;
        keep_unreadable_file(file, self.load_failed.is_set().then_some(self))?;
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer_pretty(&mut temp, &self.document())?;
        temp.write_all(b"\n")?;
        temp.as_file().sync_all()?;
        temp.persist(file).map_err(|error| error.error)?;
        self.load_failed.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use subspace_product::Channel;

    use super::{State, directory_name, directory_name_for};

    #[test]
    fn each_type_keeps_its_core_state_in_its_own_directory() {
        assert_eq!(directory_name_for(Channel::Stable), "subspace");
        assert_eq!(directory_name_for(Channel::Nightly), "subspace-nightly");
        assert_eq!(directory_name_for(Channel::Alpha), "subspace-alpha");
        assert_eq!(directory_name_for(Channel::Beta), "subspace-beta");
        assert_eq!(directory_name(), directory_name_for(Channel::current()));
    }

    #[test]
    fn values_survive_a_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("subspace").join("state.json");
        let mut state = State::default();
        state.set("board.layout", "board");
        state.set("github.collapsed", r#"["mine"]"#);
        state.save(&file).unwrap();

        state.set("board.layout", "list");
        state.remove("github.collapsed");
        state.save(&file).unwrap();

        let loaded = State::load(&file);
        assert_eq!(loaded, state);
        assert_eq!(loaded.get("board.layout"), Some("list"));
        assert_eq!(loaded.get("github.collapsed"), None);
        // Only the state file is left: the temporary file was renamed.
        assert_eq!(fs::read_dir(file.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn write_atomic_replaces_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub").join("cache.json");
        super::write_atomic(&file, "one").unwrap();
        super::write_atomic(&file, "two").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "two");
        assert_eq!(fs::read_dir(file.parent().unwrap()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_cache_replaces_permissive_files_with_private_files() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cache.json");
        fs::write(&file, "old").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        super::write_atomic(&file, "private").unwrap();
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn a_missing_or_broken_file_gives_the_defaults_and_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        assert_eq!(State::load(&file), State::default());

        fs::write(&file, "{ broken").unwrap();
        let mut state = State::load(&file);
        assert!(state.values.is_empty() && state.invalid.is_empty());
        state.set("wrap", "true");
        state.save(&file).unwrap();
        assert_eq!(State::load(&file).get("wrap"), Some("true"));
    }

    fn saved_json(file: &std::path::Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
    }

    #[test]
    fn a_non_string_entry_is_kept_and_other_values_stay_usable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        fs::write(
            &file,
            r#"{"wrap":"true","example.dock-layout":{"version":1},"count":3}"#,
        )
        .unwrap();

        let mut state = State::load(&file);
        assert_eq!(state.get("wrap"), Some("true"));
        assert_eq!(state.get("example.dock-layout"), None);
        assert!(state.has_invalid_value("example.dock-layout"));
        assert!(state.has_invalid_value("count"));
        assert!(!state.has_invalid_value("wrap"));
        assert!(!state.has_invalid_value("missing"));

        state.set("board.layout", "list");
        state.save(&file).unwrap();

        assert_eq!(
            saved_json(&file),
            serde_json::json!({
                "wrap": "true",
                "board.layout": "list",
                "example.dock-layout": {"version": 1},
                "count": 3,
            })
        );
        assert!(State::load(&file).has_invalid_value("example.dock-layout"));
        // A readable file needs no copy.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn set_and_remove_explicitly_replace_a_non_string_entry() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        fs::write(&file, r#"{"a":[1],"b":null,"keep":"yes"}"#).unwrap();

        let mut state = State::load(&file);
        state.set("a", "reset");
        state.remove("b");
        assert!(!state.has_invalid_value("a"));
        assert!(!state.has_invalid_value("b"));
        assert_eq!(state.get("a"), Some("reset"));
        state.save(&file).unwrap();

        assert_eq!(
            saved_json(&file),
            serde_json::json!({"a": "reset", "keep": "yes"})
        );
    }

    #[test]
    fn a_file_that_is_not_a_json_object_is_copied_aside_before_it_is_replaced() {
        for broken in ["{ broken", "[\"not\",\"an object\"]"] {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("state.json");
            fs::write(&file, broken).unwrap();

            let mut state = State::load(&file);
            assert!(state.values.is_empty() && state.invalid.is_empty());
            state.set("wrap", "true");
            state.save(&file).unwrap();

            assert_eq!(State::load(&file).get("wrap"), Some("true"));
            assert_eq!(copy_contents(dir.path()), [broken]);
        }
    }

    /// The contents of the `state.json.invalid-*` copies in `dir`, oldest name first.
    fn copy_contents(dir: &std::path::Path) -> Vec<String> {
        let mut copies: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("state.json.invalid-"))
            })
            .collect();
        copies.sort();
        copies
            .iter()
            .map(|path| fs::read_to_string(path).unwrap())
            .collect()
    }

    #[test]
    fn a_file_unreadable_at_load_but_valid_at_save_is_copied_before_it_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        // Reading a directory fails with an error other than "not found".
        fs::create_dir(&file).unwrap();
        let mut state = State::load(&file);
        assert!(state.values.is_empty() && state.invalid.is_empty());
        fs::remove_dir(&file).unwrap();
        let original = r#"{"keep":"yes","count":3}"#;
        fs::write(&file, original).unwrap();

        state.set("a", "b");
        state.save(&file).unwrap();
        assert_eq!(saved_json(&file), serde_json::json!({"a": "b"}));
        assert_eq!(copy_contents(dir.path()), [original]);

        // Saving the same document again loses nothing: no further copy.
        state.save(&file).unwrap();
        assert_eq!(copy_contents(dir.path()), [original]);

        // The first successful save ended the check: later changes make no copy.
        state.set("a", "c");
        state.save(&file).unwrap();
        assert_eq!(saved_json(&file), serde_json::json!({"a": "c"}));
        assert_eq!(copy_contents(dir.path()), [original]);
    }

    #[test]
    fn non_string_entries_keep_their_exact_json_text() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        let numbers = [
            "18446744073709551617",
            "-9223372036854775809",
            "1.000000000000000000001",
            "0.1000000000000000055511151231257827",
        ];
        fs::write(
            &file,
            format!(
                r#"{{"u":{},"i":{},"d":{},"f":{},"nested":{{"n":{}}},"keep":"x"}}"#,
                numbers[0], numbers[1], numbers[2], numbers[3], numbers[0]
            ),
        )
        .unwrap();

        let mut state = State::load(&file);
        assert!(state.has_invalid_value("u"));
        state.set("other", "y");
        state.save(&file).unwrap();

        let saved = fs::read_to_string(&file).unwrap();
        for number in numbers {
            assert!(saved.contains(number), "{number} missing from {saved}");
        }
        assert!(saved.contains(&format!(r#"{{"n":{}}}"#, numbers[0])));
        assert_eq!(State::load(&file), state);
        // The file was already valid: no copy.
        assert!(copy_contents(dir.path()).is_empty());
    }

    #[test]
    fn only_json_whitespace_makes_a_file_blank() {
        for content in ["\u{c}", "\u{b}", " \u{c}\n", "\u{b}\t"] {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("state.json");
            fs::write(&file, content).unwrap();

            let mut state = State::load(&file);
            state.set("a", "b");
            state.save(&file).unwrap();

            assert_eq!(copy_contents(dir.path()), [content]);
        }
        for content in [" ", "\t\r\n ", ""] {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("state.json");
            fs::write(&file, content).unwrap();
            State::load(&file).save(&file).unwrap();
            assert!(copy_contents(dir.path()).is_empty());
        }
    }

    #[test]
    fn a_normal_load_never_makes_a_copy() {
        for content in [None, Some(""), Some(" \n"), Some(r#"{"a":"1","n":2}"#)] {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("state.json");
            if let Some(content) = content {
                fs::write(&file, content).unwrap();
            }
            let mut state = State::load(&file);
            state.set("a", "x");
            state.save(&file).unwrap();
            state.set("a", "y");
            state.save(&file).unwrap();

            assert!(copy_contents(dir.path()).is_empty());
            assert_eq!(State::load(&file).get("a"), Some("y"));
        }
    }

    #[test]
    fn successive_copies_never_replace_an_earlier_copy() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        for broken in ["{ first", "{ second", "{ third"] {
            fs::write(&file, broken).unwrap();
            // The same stamp each time forces the name collision.
            super::keep_unreadable_file_stamped(&file, 7, None).unwrap();
        }
        for (index, broken) in ["{ first", "{ second", "{ third"].into_iter().enumerate() {
            let copy = dir.path().join(format!("state.json.invalid-7-{index}"));
            assert_eq!(fs::read_to_string(copy).unwrap(), broken);
        }
        // The state file and three copies; no temporary file is left.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_is_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        fs::write(&file, "{ broken").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        super::keep_unreadable_file_stamped(&file, 1, None).unwrap();
        let copy = dir.path().join("state.json.invalid-1-0");
        assert_eq!(
            fs::metadata(copy).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn a_file_that_cannot_be_inspected_is_not_treated_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        // Reading a directory fails with an error other than "not found".
        let file = dir.path().join("state.json");
        fs::create_dir(&file).unwrap();
        assert!(super::keep_unreadable_file(&file, None).is_err());
        assert!(State::default().save(&file).is_err());
        assert!(file.is_dir());

        let missing = dir.path().join("absent.json");
        assert!(super::keep_unreadable_file(&missing, None).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_copy_fails_the_save_and_keeps_the_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state.json");
        fs::write(&file, "{ broken").unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        // Running as root ignores directory permissions: nothing to prove.
        let can_write = tempfile::NamedTempFile::new_in(dir.path()).is_ok();
        let result = State::default().save(&file);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

        if !can_write {
            assert!(result.is_err());
            assert_eq!(fs::read_to_string(&file).unwrap(), "{ broken");
        }
    }
}
