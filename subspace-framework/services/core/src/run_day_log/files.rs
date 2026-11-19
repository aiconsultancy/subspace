//! Regular-file IO. Live truncation deliberately uses a writable Windows handle.

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};

#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o600;

pub(super) fn directory(path: &Path) -> io::Result<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .or(Some(Path::new(".")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log path has no parent"))
}

pub(super) fn regular(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(Some(metadata))
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("log path {} is not a regular file", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn size(path: &Path) -> io::Result<u64> {
    Ok(regular(path)?.map_or(0, |metadata| metadata.len()))
}

pub(super) fn prepare(path: &Path, limit: u64) -> io::Result<()> {
    if limit == 0 || path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid log limits or path",
        ));
    }
    let parent = directory(path)?;
    match fs::symlink_metadata(parent) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid log directory",
            ));
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    fs::create_dir_all(parent)?;
    open_private(path).map(drop)
}

pub(super) fn open_private(path: &Path) -> io::Result<File> {
    let existing = regular(path)?.is_some();
    let mut options = OpenOptions::new();
    options.append(true).read(true);
    if !existing {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(PRIVATE_MODE);
    }
    let file = match options.open(path) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => open_existing(path)?,
        result => result?,
    };
    crate::private_file::set_owner_only(path)?;
    Ok(file)
}

fn open_existing(path: &Path) -> io::Result<File> {
    regular(path)?;
    OpenOptions::new().append(true).read(true).open(path)
}

pub(super) fn truncate(path: &Path) -> io::Result<()> {
    if regular(path)?.is_some() {
        OpenOptions::new().write(true).open(path)?.set_len(0)?;
    }
    Ok(())
}

pub(super) fn replace(path: &Path, chunk: &[u8]) -> io::Result<()> {
    let mut file = open_private(path)?;
    truncate(path)?;
    file.write_all(chunk)?;
    file.flush()
}

pub(super) fn append(path: &Path, line: &[u8], limit: u64) -> io::Result<()> {
    let mut file = open_private(path)?;
    let before = file.metadata()?.len();
    if before > limit.saturating_sub(line.len() as u64) {
        return Err(io::Error::other("log chunk could not be bounded"));
    }
    if let Err(error) = file.write_all(line).and_then(|()| file.flush()) {
        let _ = OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.set_len(before));
        return Err(error);
    }
    Ok(())
}

pub(super) fn read_chunk(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    open_optional(path)?.map_or_else(|| Ok(Vec::new()), |file| read_limited(file, limit))
}

pub(super) fn open_optional(path: &Path) -> io::Result<Option<File>> {
    regular(path)?.map(|_| File::open(path)).transpose()
}

fn read_limited(file: File, limit: u64) -> io::Result<Vec<u8>> {
    let mut chunk = Vec::new();
    file.take(read_limit(limit)).read_to_end(&mut chunk)?;
    check_chunk_size(&chunk, limit)?;
    Ok(chunk)
}

fn read_limit(limit: u64) -> u64 {
    limit.saturating_add(1)
}

fn check_chunk_size(chunk: &[u8], limit: u64) -> io::Result<()> {
    if chunk.len() as u64 > limit {
        return Err(io::Error::other("log chunk exceeds the byte limit"));
    }
    Ok(())
}

pub(super) fn remove(path: &Path) -> io::Result<()> {
    if regular(path)?.is_some() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(super) fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

pub(super) fn bound_existing(live: &Path, limit: u64) -> io::Result<()> {
    for (_, path) in super::retention::dates(live)? {
        if size(&path)? > limit {
            let chunk = super::migration::bounded_records(&path, limit)?;
            replace(&path, &chunk)?;
        }
    }
    if size(live)? > limit {
        let chunk = super::migration::bounded_records(live, limit)?;
        super::marker::reset(live, || replace(live, &chunk))?;
    }
    Ok(())
}
