//! Windows file identity from an open handle.
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle as _;

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO, FileIdInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx,
};

use super::{FileId, Kind};

/// Returns the 128-bit file ID. Falls back to the legacy 64-bit file index
/// only when the file system does not support `FileIdInfo`; any other error
/// fails closed. ReFS, where the legacy index is not unique, supports
/// `FileIdInfo`, so it never uses the fallback.
pub(super) fn file_id(file: &File) -> io::Result<FileId> {
    match id_info(file) {
        Err(error) if file_id_info_unsupported(error.raw_os_error()) => handle_info(file),
        result => result,
    }
}

/// Whether a `FileIdInfo` query failed only because the file system lacks it.
fn file_id_info_unsupported(code: Option<i32>) -> bool {
    code.and_then(|code| u32::try_from(code).ok())
        .is_some_and(|code| {
            [
                ERROR_INVALID_PARAMETER,
                ERROR_INVALID_FUNCTION,
                ERROR_NOT_SUPPORTED,
            ]
            .contains(&code)
        })
}

// qual:allow(unsafe) reason: "Audited Windows file-information query on a borrowed open handle into a correctly sized local buffer."
fn id_info(file: &File) -> io::Result<FileId> {
    let mut info = FILE_ID_INFO::default();
    let size = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: the handle is owned by `file` and stays open for the call; `info`
    // is a writable FILE_ID_INFO of exactly `size` bytes, as FileIdInfo requires.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            std::ptr::from_mut(&mut info).cast(),
            size,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(FileId {
        kind: Kind::FileIdInfo,
        volume: info.VolumeSerialNumber,
        file: u128::from_le_bytes(info.FileId.Identifier),
    })
}

// qual:allow(unsafe) reason: "Audited Windows file-information query on a borrowed open handle into a local output structure."
fn handle_info(file: &File) -> io::Result<FileId> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is owned by `file` and stays open for the call; `info`
    // is a writable BY_HANDLE_FILE_INFORMATION.
    let ok =
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), std::ptr::from_mut(&mut info)) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let index = (u64::from(info.nFileIndexHigh) << u32::BITS) | u64::from(info.nFileIndexLow);
    Ok(FileId {
        kind: Kind::FileIndex,
        volume: u64::from(info.dwVolumeSerialNumber),
        file: u128::from(index),
    })
}

#[cfg(test)]
mod tests {
    use super::super::{FileId, Kind};
    use super::file_id_info_unsupported;
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION, ERROR_INVALID_HANDLE, ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED,
    };

    fn code(error: u32) -> Option<i32> {
        Some(i32::try_from(error).expect("error code fits"))
    }

    #[test]
    fn only_unsupported_file_id_info_uses_the_legacy_index() {
        for error in [
            ERROR_INVALID_PARAMETER,
            ERROR_INVALID_FUNCTION,
            ERROR_NOT_SUPPORTED,
        ] {
            assert!(file_id_info_unsupported(code(error)), "{error}");
        }
        for error in [ERROR_ACCESS_DENIED, ERROR_INVALID_HANDLE] {
            assert!(!file_id_info_unsupported(code(error)), "{error}");
        }
        assert!(!file_id_info_unsupported(None));
        assert!(!file_id_info_unsupported(Some(-1)));
    }

    #[test]
    fn formats_never_compare_equal() {
        let id = |kind| FileId {
            kind,
            volume: 1,
            file: 2,
        };
        assert_ne!(id(Kind::FileIdInfo), id(Kind::FileIndex));
    }
}
