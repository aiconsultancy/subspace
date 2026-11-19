//! Fixed public model supply. No hub client, credentials, or environment model URLs.
use std::{fs, io::Read, path::Path};

use sha2::{Digest, Sha256};

mod download;
mod manifest;
mod staging;
use manifest::{ASSETS, Asset};
pub(crate) use manifest::{DIRECTORY, NAME};

const HASH_BUFFER_BYTES: usize = 65_536;
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;

pub(crate) fn total_bytes() -> u64 {
    ASSETS.iter().map(|asset| asset.bytes).sum()
}

pub(crate) fn present(root: &Path) -> bool {
    no_links(root).is_ok()
        && ASSETS.iter().all(|asset| {
            fs::symlink_metadata(root.join(asset.name))
                .is_ok_and(|meta| meta.is_file() && meta.len() == asset.bytes)
        })
}

pub(crate) fn validate(root: &Path) -> Result<(), String> {
    no_links(root)?;
    for asset in ASSETS {
        validate_asset(root, asset)?;
    }
    Ok(())
}

fn validate_asset(root: &Path, asset: &Asset) -> Result<(), String> {
    let path = root.join(asset.name);
    let meta = fs::symlink_metadata(&path)
        .map_err(|_| "Local model is missing; install it explicitly.")?;
    if !meta.is_file() || meta.len() != asset.bytes {
        return Err("Local model asset is invalid; reinstall it.".into());
    }
    let file = fs::File::open(path).map_err(|_| "Cannot read local model asset.")?;
    verify_reader(file, asset)
}

fn verify_reader(mut reader: impl Read, asset: &Asset) -> Result<(), String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER_BYTES];
    let mut length = 0u64;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "Cannot read model asset.")?;
        if count == 0 {
            break;
        }
        length += count as u64;
        if length > asset.bytes {
            return Err("Model asset exceeds its fixed size.".into());
        }
        hash.update(&buffer[..count]);
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if length != asset.bytes || digest != asset.sha256 {
        return Err("Model asset checksum does not match the pinned revision.".into());
    }
    Ok(())
}

pub(crate) fn install(root: &Path) -> Result<(), String> {
    private_directory(root)?;
    let _installer = staging::Installer::acquire(root)?;
    let agent = download::agent();
    for asset in ASSETS {
        if validate_asset(root, asset).is_ok() {
            continue;
        }
        download::asset(&agent, root, asset)?;
    }
    validate(root)
}

fn no_links(root: &Path) -> Result<(), String> {
    for ancestor in root.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err("Model cache must not use symbolic links.".into());
        }
    }
    Ok(())
}

pub(crate) fn private_directory(root: &Path) -> Result<(), String> {
    no_links(root)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(PRIVATE_DIRECTORY_MODE);
    }
    builder
        .create(root)
        .map_err(|_| "Cannot create private model cache.")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(|_| "Cannot protect model cache.")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::private_directory;
    use super::{Asset, verify_reader};

    #[test]
    fn checksums_and_bounds_reject_corruption() {
        let asset = Asset {
            name: "fixture",
            remote: "fixture",
            bytes: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        };
        assert!(verify_reader(&b"abc"[..], &asset).is_ok());
        assert!(verify_reader(&b"abd"[..], &asset).is_err());
        assert!(verify_reader(&b"ab"[..], &asset).is_err());
        assert!(verify_reader(&b"abcd"[..], &asset).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn private_cache_rejects_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        assert!(private_directory(&link.join("model")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_directories() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("model");
        private_directory(&root).unwrap();
        assert_eq!(
            std::fs::metadata(root).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
