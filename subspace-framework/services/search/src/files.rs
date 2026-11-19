use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::storage_error;

#[cfg(unix)]
const PRIVATE_DIRECTORY: u32 = 0o700;
#[cfg(unix)]
const PRIVATE_FILE: u32 = 0o600;

pub(crate) fn directory(root: &Path, app: &str) -> Result<PathBuf, String> {
    let path = root.join(hex(app));
    private_directory(root)?;
    private_directory(&path)?;
    Ok(path)
}

fn private_directory(path: &Path) -> Result<(), String> {
    if path
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err("Search storage cannot be a symbolic link".into());
    }
    fs::create_dir_all(path).map_err(storage_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY))
            .map_err(storage_error)?;
    }
    Ok(())
}

pub(crate) fn database_path(root: &Path, board: &str) -> Result<PathBuf, String> {
    let path = root.join(format!("{}.sqlite", hex(board)));
    if path.symlink_metadata().is_ok_and(|meta| !meta.is_file()) {
        return Err("Search database must be a regular private file".into());
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE);
    }
    options.open(&path).map_err(storage_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(PRIVATE_FILE))
            .map_err(storage_error)?;
    }
    Ok(path)
}

fn hex(text: &str) -> String {
    use std::fmt::Write;
    text.as_bytes()
        .iter()
        .fold(String::new(), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        })
}

pub(crate) fn usage(path: &Path) -> Result<u64, String> {
    ["", "-journal", "-wal", "-shm", "-revision"]
        .iter()
        .try_fold(0_u64, |total, suffix| {
            let mut file = path.as_os_str().to_os_string();
            file.push(suffix);
            match fs::metadata(file) {
                Ok(meta) => Ok(total.saturating_add(allocated(&meta))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(total),
                Err(error) => Err(storage_error(error)),
            }
        })
}

fn allocated(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.len().max(meta.blocks().saturating_mul(512))
    }
    #[cfg(not(unix))]
    {
        meta.len().div_ceil(65536).saturating_mul(65536)
    }
}
