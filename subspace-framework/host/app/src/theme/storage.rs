//! Named theme files with atomic writes and scoped writer exclusion.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1024 * 1024;
const READ_LIMIT: u64 = MAX_BYTES + 1;
const NAME_MAX: usize = 80;
const TEMP_ATTEMPTS: u32 = 16;
const SAVE_LOCK: &str = ".save.lock";

/// Portable `{name}.json` for a product-local custom theme.
pub fn theme_filename(name: &str) -> Result<String, String> {
    validate_name(name)?;
    Ok(format!("{name}.json"))
}

pub fn read_theme(path: &Path) -> Result<String, String> {
    let meta = inspect_file(path)?;
    refuse_oversize(path, meta.len())?;
    read_bounded(path)
}

pub fn target(directory: &Path, name: &str) -> Result<PathBuf, String> {
    Ok(directory.join(theme_filename(name)?))
}

pub fn save(
    directory: &Path,
    name: &str,
    content: &str,
    expected: Option<&str>,
) -> Result<PathBuf, String> {
    if content.len() as u64 > MAX_BYTES {
        return Err("theme file is too large".into());
    }
    let path = target(directory, name)?;
    ensure_directory(directory)?;
    let _lock = acquire_lock(directory)?;
    validate_non_target_names(directory, &path, name)?;
    match expected {
        None => create_only(&path, content)?,
        Some(snapshot) => overwrite_if_matches(&path, content, snapshot)?,
    }
    Ok(path)
}

struct WriterLock {
    path: PathBuf,
    file: Option<File>,
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        self.file = None;
        let _ = fs::remove_file(&self.path);
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("theme name is not portable".into());
    }
    if name.len() > NAME_MAX {
        return Err("theme name is too long".into());
    }
    if name.starts_with(' ') || name.ends_with(' ') || name.ends_with('.') {
        return Err("theme name is not portable".into());
    }
    if !name.chars().all(is_portable_char) {
        return Err("theme name is not portable".into());
    }
    if is_windows_device(name) {
        return Err("theme name is a reserved Windows device name".into());
    }
    Ok(())
}

fn is_portable_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-'
}

fn is_windows_device(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    matches!(name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || numbered_device(&name, "COM")
        || numbered_device(&name, "LPT")
}

fn numbered_device(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    matches!(rest, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
}

fn inspect_file(path: &Path) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{} is a symbolic link", path.display()));
    }
    if !meta.file_type().is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(meta)
}

fn refuse_oversize(path: &Path, len: u64) -> Result<(), String> {
    if len > MAX_BYTES {
        Err(format!("{} is too large", path.display()))
    } else {
        Ok(())
    }
}

fn ensure_directory(directory: &Path) -> Result<(), String> {
    match fs::symlink_metadata(directory) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(format!("{} is a symbolic link", directory.display()))
        }
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(format!("{} is not a directory", directory.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(directory).map_err(|error| io_error(directory, error))
        }
        Err(error) => Err(io_error(directory, error)),
    }
}

fn acquire_lock(directory: &Path) -> Result<WriterLock, String> {
    let lock_path = directory.join(SAVE_LOCK);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|_| "another save is in progress".to_string())?;
    Ok(WriterLock {
        path: lock_path,
        file: Some(file),
    })
}

fn validate_non_target_names(directory: &Path, path: &Path, name: &str) -> Result<(), String> {
    let wanted = theme_filename(name)?;
    for other in theme_files(directory)? {
        if other == path {
            continue;
        }
        refuse_filename_clash(&other, &wanted)?;
        refuse_variant_clash(&other, name)?;
    }
    Ok(())
}

fn theme_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(directory, error)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| io_error(directory, error))?.path();
        if is_theme_json(&path) {
            files.push(path);
        }
    }
    Ok(files)
}

fn is_theme_json(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("json")
}

fn refuse_filename_clash(other: &Path, wanted: &str) -> Result<(), String> {
    let Some(file_name) = other.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    if file_name.eq_ignore_ascii_case(wanted) {
        Err("That name belongs to another theme file. Choose a new name.".into())
    } else {
        Ok(())
    }
}

fn refuse_variant_clash(other: &Path, name: &str) -> Result<(), String> {
    let Ok(text) = read_theme(other) else {
        return Ok(());
    };
    let Ok(document) = super::ThemeDocument::parse(&text) else {
        return Ok(());
    };
    if document
        .names()
        .iter()
        .any(|variant| variant.eq_ignore_ascii_case(name))
    {
        Err("That name belongs to another theme file. Choose a new name.".into())
    } else {
        Ok(())
    }
}

fn read_bounded(path: &Path) -> Result<String, String> {
    let text = read_limited(path).map_err(|error| io_error(path, error))?;
    refuse_oversize(path, text.len() as u64)?;
    Ok(text)
}

fn read_limited(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    File::open(path)?
        .take(READ_LIMIT)
        .read_to_string(&mut text)?;
    Ok(text)
}

fn create_only(path: &Path, content: &str) -> Result<(), String> {
    let temp = write_temp_for(path, content)?;
    match fs::hard_link(&temp, path) {
        Ok(()) => {
            remove_temp(&temp);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            remove_temp(&temp);
            Err(format!("{} already exists", path.display()))
        }
        Err(error) => {
            remove_temp(&temp);
            Err(io_error(path, error))
        }
    }
}

fn overwrite_if_matches(path: &Path, content: &str, expected: &str) -> Result<(), String> {
    inspect_file(path)?;
    let temp = write_temp_for(path, content)?;
    match read_theme(path) {
        Ok(current) if current == expected => persist_temp(&temp, path),
        Ok(_) => {
            remove_temp(&temp);
            Err(format!("{} has changed", path.display()))
        }
        Err(error) => {
            remove_temp(&temp);
            Err(error)
        }
    }
}

fn write_temp_for(path: &Path, content: &str) -> Result<PathBuf, String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("{} has no directory", path.display()))?;
    create_temp(directory, path, content.as_bytes())
}

fn remove_temp(path: &Path) {
    let _ = fs::remove_file(path);
}

fn create_temp(directory: &Path, path: &Path, data: &[u8]) -> Result<PathBuf, String> {
    let base = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "theme.json".into());
    for attempt in 0..TEMP_ATTEMPTS {
        let temp = directory.join(format!(
            "{base}.tmp-{}-{}-{attempt}",
            std::process::id(),
            now_nanos()
        ));
        match write_temp(&temp, data) {
            Ok(()) => return Ok(temp),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error(&temp, error)),
        }
    }
    Err(format!(
        "could not create a temporary file in {}",
        directory.display()
    ))
}

fn write_temp(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

fn persist_temp(temp: &Path, path: &Path) -> Result<(), String> {
    fs::rename(temp, path).map_err(|error| {
        remove_temp(temp);
        io_error(path, error)
    })
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn io_error(path: &Path, error: io::Error) -> String {
    format!("{}: {error}", path.display())
}

#[cfg(test)]
mod tests {
    use super::{MAX_BYTES, read_theme, save, target, theme_filename};
    use std::fs;

    #[test]
    fn theme_filename_accepts_portable_names() {
        assert_eq!(theme_filename("My Theme").unwrap(), "My Theme.json");
        assert_eq!(theme_filename("a_b-1").unwrap(), "a_b-1.json");
        assert_eq!(
            target(std::path::Path::new("/themes"), "My Theme").unwrap(),
            std::path::PathBuf::from("/themes/My Theme.json")
        );
    }

    #[test]
    fn theme_filename_rejects_paths_devices_and_length() {
        assert!(theme_filename("../escape").is_err());
        assert!(theme_filename("a/b").is_err());
        assert!(theme_filename("CON").is_err());
        assert!(theme_filename("com1").is_err());
        assert!(theme_filename("hello.").is_err());
        assert!(theme_filename("hello ").is_err());
        assert!(theme_filename("").is_err());
        assert!(theme_filename(&"a".repeat(81)).is_err());
        assert!(theme_filename(&"a".repeat(80)).is_ok());
    }

    #[test]
    fn save_creates_and_overwrites_with_a_content_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = save(dir.path(), "Mine", "one", None).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
        assert!(save(dir.path(), "Mine", "two", None).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(save(dir.path(), "Mine", "two", Some("stale")).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(save(dir.path(), "Mine", "two", Some("one")).unwrap(), path);
        assert_eq!(read_theme(&path).unwrap(), "two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn save_refuses_a_competing_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join(".save.lock");
        fs::write(&lock, "").unwrap();
        assert!(save(dir.path(), "Mine", "one", None).is_err());
        fs::remove_file(lock).unwrap();
        save(dir.path(), "Mine", "one", None).unwrap();
    }

    fn named_theme(name: &str) -> String {
        serde_json::json!({
            "name": name,
            "themes": [{
                "name": name,
                "mode": "dark",
                "colors": { "background": "#101115" }
            }]
        })
        .to_string()
    }

    #[test]
    fn save_refuses_a_variant_owned_by_another_file() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("Other.json");
        fs::write(&other, named_theme("Mine")).unwrap();
        assert!(save(dir.path(), "Mine", &named_theme("Mine"), None).is_err());
        assert_eq!(fs::read_to_string(&other).unwrap(), named_theme("Mine"));
        assert!(!dir.path().join("Mine.json").exists());
    }

    #[test]
    fn save_refuses_overwrite_when_another_file_has_the_variant() {
        let dir = tempfile::tempdir().unwrap();
        let original = named_theme("Mine");
        let path = save(dir.path(), "Mine", &original, None).unwrap();
        fs::write(dir.path().join("Other.json"), named_theme("Mine")).unwrap();
        assert!(save(dir.path(), "Mine", &named_theme("Mine"), Some(&original)).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn save_refuses_a_differently_cased_filename() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("mine.json");
        fs::write(&existing, named_theme("Other")).unwrap();
        assert!(save(dir.path(), "Mine", &named_theme("Mine"), None).is_err());
        assert_eq!(fs::read_to_string(&existing).unwrap(), named_theme("Other"));
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["mine.json"]);
    }

    #[test]
    fn read_theme_refuses_oversize_and_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.json");
        assert!(read_theme(&missing).is_err());
        let huge = dir.path().join("huge.json");
        fs::write(&huge, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        assert!(read_theme(&huge).is_err());
        assert!(
            save(
                dir.path(),
                "Huge",
                &"x".repeat(MAX_BYTES as usize + 1),
                None
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_and_save_refuse_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.json");
        fs::write(&real, "one").unwrap();
        let link = dir.path().join("Mine.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(read_theme(&link).is_err());
        assert!(save(dir.path(), "Mine", "two", Some("one")).is_err());
        assert_eq!(fs::read_to_string(&real).unwrap(), "one");
        let nested = dir.path().join("real");
        fs::create_dir(&nested).unwrap();
        let dir_link = dir.path().join("link");
        std::os::unix::fs::symlink(&nested, &dir_link).unwrap();
        assert!(save(&dir_link, "Mine", "one", None).is_err());
        assert_eq!(fs::read_dir(&nested).unwrap().count(), 0);
    }
}
