//! One persistent lock inode and six deterministic staging slots, including after a crash.
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use super::manifest::ASSETS;

const LOCK_FILE: &str = ".install.lock";
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

pub(super) struct Installer {
    // Closing the file releases the OS lock even if the process is terminated.
    _lock: File,
}

impl Installer {
    pub(super) fn acquire(root: &Path) -> Result<Self, String> {
        let lock = lock_file(&root.join(LOCK_FILE))?;
        lock.try_lock()
            .map_err(|_| "Another process is installing the local model.")?;
        recover(root)?;
        Ok(Self { _lock: lock })
    }
}

fn lock_file(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(PRIVATE_FILE_MODE)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|_| "Cannot safely open the model installation lock.")?;
    if !file
        .metadata()
        .map_err(|_| "Cannot inspect model lock.")?
        .is_file()
    {
        return Err("Model installation lock must be a regular file.".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if file
            .metadata()
            .map_err(|_| "Cannot inspect model lock.")?
            .file_attributes()
            & FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err("Model installation lock must not be a reparse point.".into());
        }
    }
    Ok(file)
}

fn recover(root: &Path) -> Result<(), String> {
    for asset in ASSETS {
        remove_stale(&root.join(format!(".{}.part", asset.name)))?;
    }
    Ok(())
}

fn remove_stale(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => {
            fs::remove_file(path).map_err(|_| "Cannot recover abandoned model staging file.".into())
        }
        Ok(_) => Err("Model staging slot must be a regular file, not a link or directory.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot inspect model staging slot.".into()),
    }
}

pub(super) struct Stage {
    pub(super) file: File,
    path: PathBuf,
}

impl Stage {
    pub(super) fn create(root: &Path, name: &str) -> Result<Self, String> {
        let path = root.join(format!(".{name}.part"));
        let mut options = OpenOptions::new();
        // create_new atomically rejects any existing entry, including symlinks.
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(PRIVATE_FILE_MODE);
        }
        let file = options
            .open(&path)
            .map_err(|_| "Cannot safely create model staging slot.")?;
        Ok(Self { file, path })
    }

    pub(super) fn commit(self, target: &Path) -> Result<(), String> {
        self.file
            .sync_all()
            .map_err(|_| "Cannot persist model asset.")?;
        fs::rename(&self.path, target).map_err(|_| "Cannot install verified model asset.".into())
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests;
