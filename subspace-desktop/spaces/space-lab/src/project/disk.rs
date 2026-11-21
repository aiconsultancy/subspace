//! Bounded, no-symlink project I/O. The application-owned root is canonical;
//! every descendant is checked before use. Local hostile directory replacement
//! is outside the worker boundary; workers never get writable store access.
use anyhow::{Context, Result, bail, ensure};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a project write waits for the store lock before it reports a
/// concurrent writer.
const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_RETRY: Duration = Duration::from_millis(10);

/// Creates a directory, or accepts a real one that is already there. Creating
/// before checking lets concurrent openers of one store race safely: the loser
/// gets `AlreadyExists` and checks what the winner made. A symlink, dangling
/// or not, or a file at the path also gives `AlreadyExists` and is rejected.
pub(super) fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "unsafe directory: {}",
        path.display()
    );
    Ok(())
}

pub(super) fn descendant(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for segment in relative.split('/') {
        path.push(segment);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "symlink rejected: {}",
            path.display()
        );
    }
    ensure!(
        path.canonicalize()?.starts_with(root),
        "source escapes project root"
    );
    Ok(path)
}

pub(super) fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    check_regular_file(path, limit)?;
    bounded_read(path, limit)
}

fn check_regular_file(path: &Path, limit: usize) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "expected a regular non-symlink file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= limit as u64,
        "file exceeds {limit} bytes: {}",
        path.display()
    );
    Ok(())
}

fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "file exceeds {limit} bytes");
    Ok(bytes)
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("file has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    // Windows has no directory fsync in std; file data is flushed before replacement.
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub(super) fn write_tree(
    root: &Path,
    files: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for (relative, bytes) in files {
        let path = root.join(relative);
        let parent = path.parent().context("source has no parent")?;
        fs::create_dir_all(parent)?;
        atomic_write(&path, bytes)?;
    }
    Ok(())
}

pub(super) fn lock(root: &Path) -> Result<File> {
    let path = root.join(".project-write.lock");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "unsafe project lock"
        );
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    // A child that another thread is spawning inherits this lock's open file
    // description until it execs, so a lock released here can stay held for
    // a moment. Wait briefly before reporting a real concurrent writer.
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(LOCK_RETRY);
            }
            Err(TryLockError::WouldBlock) => bail!("another project write is in progress"),
            Err(TryLockError::Error(error)) => {
                return Err(error).context("cannot lock the project store");
            }
        }
    }
}
