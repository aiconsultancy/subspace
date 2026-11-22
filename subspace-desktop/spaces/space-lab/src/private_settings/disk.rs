use super::{Metadata, validate};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

// JSON escaping can expand each input byte by six, plus bounded names and syntax.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_READ_BYTES: u64 = MAX_FILE_BYTES + 1;
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub revision: u64,
    pub values: BTreeMap<String, String>,
}

impl Record {
    pub fn metadata(&self) -> Metadata {
        Metadata {
            revision: self.revision,
            names: self.values.keys().cloned().collect(),
        }
    }
}

pub(super) fn prepare(root: &Path, id: &str) -> Result<PathBuf> {
    ensure!(
        fs::symlink_metadata(root)?.file_type().is_dir(),
        "invalid board root"
    );
    let root = root.canonicalize()?;
    let private = root.join("private");
    directory(&private)?;
    let project = private.join(id);
    directory(&project)?;
    Ok(project)
}

#[cfg(unix)]
fn directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    match fs::DirBuilder::new()
        .mode(PRIVATE_DIRECTORY_MODE)
        .create(path)
    {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_dir()
            && metadata.permissions().mode() & 0o7777 == PRIVATE_DIRECTORY_MODE,
        "unsafe private directory"
    );
    Ok(())
}

#[cfg(not(unix))]
fn directory(_path: &Path) -> Result<()> {
    anyhow::bail!("private storage permissions unavailable on this platform")
}

fn check_directory(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid private directory"))?;
    check_existing_directory(parent)?;
    check_existing_directory(path)
}

#[cfg(unix)]
fn check_existing_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_dir()
            && metadata.permissions().mode() & 0o7777 == PRIVATE_DIRECTORY_MODE,
        "unsafe private directory"
    );
    Ok(())
}

#[cfg(not(unix))]
fn check_existing_directory(_path: &Path) -> Result<()> {
    anyhow::bail!("private storage permissions unavailable on this platform")
}

#[cfg(unix)]
fn checked_file(path: &Path) -> Result<Option<fs::Metadata>> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.file_type().is_file()
            && metadata.permissions().mode() & 0o7777 == PRIVATE_FILE_MODE,
        "unsafe private file"
    );
    Ok(Some(metadata))
}

#[cfg(not(unix))]
fn checked_file(_path: &Path) -> Result<Option<fs::Metadata>> {
    anyhow::bail!("private storage permissions unavailable on this platform")
}

pub(super) fn lock(directory: &Path) -> Result<File> {
    check_directory(directory)?;
    let path = directory.join("settings.lock");
    let existing = checked_file(&path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    if existing.is_none() {
        options.create_new(true);
    }
    let file = match options.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            checked_file(&path)?;
            OpenOptions::new().read(true).write(true).open(&path)?
        }
        Err(error) => return Err(error.into()),
    };
    file.lock()?;
    verify_open_file(&path, &file)?;
    Ok(file)
}

#[cfg(unix)]
fn verify_open_file(path: &Path, file: &File) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let path_metadata =
        checked_file(path)?.ok_or_else(|| anyhow::anyhow!("private file disappeared"))?;
    let open_metadata = file.metadata()?;
    ensure!(
        path_metadata.dev() == open_metadata.dev() && path_metadata.ino() == open_metadata.ino(),
        "private file changed during access"
    );
    Ok(())
}

#[cfg(not(unix))]
fn verify_open_file(_path: &Path, _file: &File) -> Result<()> {
    anyhow::bail!("private storage permissions unavailable on this platform")
}

pub(super) fn read(directory: &Path) -> Result<Record> {
    check_directory(directory)?;
    let path = directory.join("settings.json");
    let Some(bytes) = read_file(&path)? else {
        return Ok(Record::default());
    };
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid private settings file"))?;
    validate(&record.values)?;
    Ok(record)
}

fn read_file(path: &Path) -> Result<Option<Vec<u8>>> {
    checked_file(path)?
        .map(|metadata| read_checked_file(path, metadata))
        .transpose()
}

fn read_checked_file(path: &Path, metadata: fs::Metadata) -> Result<Vec<u8>> {
    check_size(metadata.len())?;
    let mut bytes = Vec::new();
    let file = File::open(path)?;
    verify_open_file(path, &file)?;
    file.take(MAX_READ_BYTES).read_to_end(&mut bytes)?;
    check_size(bytes.len() as u64)?;
    Ok(bytes)
}

fn check_size(size: u64) -> Result<()> {
    ensure!(size <= MAX_FILE_BYTES, "private settings file too large");
    Ok(())
}

pub(super) fn write(directory: &Path, record: &Record) -> Result<()> {
    check_directory(directory)?;
    let path = directory.join("settings.json");
    checked_file(&path)?;
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
    }
    serde_json::to_writer(&mut temp, record)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}
