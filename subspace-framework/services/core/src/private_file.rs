//! Owner-only file permissions shared by token and credential stores.
use std::path::Path;
use std::{fs, io};

#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o600;
#[cfg(unix)]
const MODE_BITS: u32 = 0o777;

#[cfg(windows)]
pub mod windows_acl;

/// Sets permissions before any secret content is written.
pub fn set_owner_only(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_MODE))
    }
    #[cfg(windows)]
    {
        windows_acl::set_owner_only(path, false).map_err(|_| {
            io::Error::new(io::ErrorKind::PermissionDenied, "cannot set owner-only ACL")
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private files are unsupported",
        ))
    }
}

pub fn is_owner_only(path: &Path) -> io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    permissions_are_private(path, &metadata)
}

#[cfg(unix)]
fn permissions_are_private(_path: &Path, metadata: &fs::Metadata) -> io::Result<bool> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    Ok(metadata.permissions().mode() & MODE_BITS == PRIVATE_MODE
        && metadata.uid() == rustix::process::geteuid().as_raw())
}

#[cfg(windows)]
fn permissions_are_private(path: &Path, _metadata: &fs::Metadata) -> io::Result<bool> {
    Ok(windows_acl::verify_owner_only(path, true, false).is_ok())
}

#[cfg(not(any(unix, windows)))]
fn permissions_are_private(_path: &Path, _metadata: &fs::Metadata) -> io::Result<bool> {
    Ok(false)
}
