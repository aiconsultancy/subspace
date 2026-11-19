//! MCP bearer tokens live outside config.json and never appear in diagnostics.
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::{Secret, private_file, state};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStatus {
    Missing,
    Private,
    NotPrivate,
}

/// Where an MCP process reads a token from besides a file.
pub const TOKEN_VARIABLE: &str = "SUBSPACE_MCP_TOKEN";

pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join("mcp-token")
}

/// Creates a new 256-bit token. The value is not returned to the UI.
pub fn create_or_replace(path: &Path) -> io::Result<()> {
    state::write_atomic(path, generate().expose())
}

/// 256 bits.
const TOKEN_BYTES: usize = 32;

/// A new 256-bit token, as hex, kept only in memory.
pub fn generate() -> Secret {
    let bytes: [u8; TOKEN_BYTES] = rand::random();
    Secret::new(hex::encode(bytes))
}

pub fn status(path: &Path) -> io::Result<TokenStatus> {
    match private_file::is_owner_only(path) {
        Ok(true) => Ok(TokenStatus::Private),
        Ok(false) => Ok(TokenStatus::NotPrivate),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(TokenStatus::Missing),
        Err(error) => Err(error),
    }
}

/// Refuses non-private, missing and empty token files.
pub fn read(path: &Path) -> io::Result<Secret> {
    if status(path)? != TokenStatus::Private {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "MCP token file is missing or not private",
        ));
    }
    let token = fs::read_to_string(path)?;
    if token.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MCP token file is empty",
        ));
    }
    Ok(Secret::new(token.trim()))
}

#[cfg(test)]
mod tests {
    use super::{TokenStatus, create_or_replace, path, read, status};

    #[test]
    fn token_is_private_random_hex_and_replaced_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let file = path(dir.path());
        assert_eq!(status(&file).unwrap(), TokenStatus::Missing);
        create_or_replace(&file).unwrap();
        assert_eq!(status(&file).unwrap(), TokenStatus::Private);
        let first = read(&file).unwrap();
        assert_eq!(first.expose().len(), 64);
        assert!(first.expose().bytes().all(|byte| byte.is_ascii_hexdigit()));
        create_or_replace(&file).unwrap();
        assert_ne!(first, read(&file).unwrap());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn readable_by_others_and_symlinks_are_not_private() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};
        let dir = tempfile::tempdir().unwrap();
        let file = path(dir.path());
        create_or_replace(&file).unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let link = dir.path().join("link");
        symlink(&file, &link).unwrap();
        assert_eq!(status(&link).unwrap(), TokenStatus::NotPrivate);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(status(&file).unwrap(), TokenStatus::NotPrivate);
        assert!(read(&file).is_err());
    }
}
