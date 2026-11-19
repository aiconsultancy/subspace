//! The identity of an open file, independent of its path and timestamps.
//!
//! A rename over a path can install a new file with identical bytes. On
//! Windows, file-system tunnelling gives the new file the old creation time,
//! and the modification time only changes when the write lands on a later
//! clock tick. The volume and file index identify the new file in each case.
use std::fs::File;
use std::io;

#[cfg(windows)]
mod windows;

/// Volume and file index of an open file: `(dev, ino)` on Unix, the volume
/// serial number and file ID on Windows. IDs of different formats never
/// compare equal, so a format change reads as a different file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId {
    kind: Kind,
    volume: u64,
    file: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    #[cfg(unix)]
    Unix,
    #[cfg(windows)]
    FileIdInfo,
    #[cfg(windows)]
    FileIndex,
}

impl FileId {
    /// Reads the identity of `file` from its open handle, so the result
    /// describes the same file as any bytes read through that handle.
    pub fn of(file: &File) -> io::Result<Self> {
        read(file)
    }
}

#[cfg(unix)]
fn read(file: &File) -> io::Result<FileId> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = file.metadata()?;
    Ok(FileId {
        kind: Kind::Unix,
        volume: metadata.dev(),
        file: u128::from(metadata.ino()),
    })
}

#[cfg(windows)]
fn read(file: &File) -> io::Result<FileId> {
    windows::file_id(file)
}

#[cfg(not(any(unix, windows)))]
fn read(_file: &File) -> io::Result<FileId> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "file identity is unsupported",
    ))
}

#[cfg(test)]
mod tests {
    use super::FileId;
    use std::fs::File;
    use std::path::Path;

    fn id(path: &Path) -> FileId {
        FileId::of(&File::open(path).expect("open")).expect("file id")
    }

    #[test]
    fn same_path_read_twice_has_the_same_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("source");
        std::fs::write(&path, b"bytes").expect("write");
        assert_eq!(id(&path), id(&path));
    }

    #[test]
    fn in_place_rewrite_keeps_the_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("source");
        std::fs::write(&path, b"bytes").expect("write");
        let before = id(&path);
        std::fs::write(&path, b"other bytes").expect("rewrite");
        assert_eq!(before, id(&path));
    }

    #[test]
    fn rename_over_with_identical_bytes_changes_the_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("source");
        let sibling = dir.path().join("replacement");
        std::fs::write(&path, b"bytes").expect("write");
        let before = id(&path);
        std::fs::write(&sibling, b"bytes").expect("write sibling");
        std::fs::rename(&sibling, &path).expect("rename");
        assert_ne!(before, id(&path));
    }

    #[test]
    fn different_files_have_different_ids() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::write(&first, b"bytes").expect("write");
        std::fs::write(&second, b"bytes").expect("write");
        assert_ne!(id(&first), id(&second));
    }
}
