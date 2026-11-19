use super::Counters;
use crate::storage_error;
use std::{
    fs::OpenOptions,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
    ptr::NonNull,
};

const MAP_BYTES: usize = 4096;
const PRIVATE_FILE: u32 = 0o600;
pub(super) const SUPPORTED: bool = true;
pub(super) struct Mapping {
    pointer: NonNull<Counters>,
}

// SAFETY: the page-aligned MAP_SHARED allocation contains only lock-free atomic
// u64 counters. All access uses atomics; the mapping lives until the last Arc drops.
unsafe impl Send for Mapping {}
// SAFETY: no non-atomic mutable references to the shared counters are created.
unsafe impl Sync for Mapping {}

impl Mapping {
    // qual:allow(complexity, unsafe) reason: "POSIX shared-memory boundary; validated mapping size/alignment and separate-process revocation tests cover its contract."
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut name = path.as_os_str().to_os_string();
        name.push("-revision");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(PRIVATE_FILE)
            .custom_flags(libc::O_NOFOLLOW)
            .open(name)
            .map_err(storage_error)?;
        let length = file.metadata().map_err(storage_error)?.len();
        if length == 0 {
            file.set_len(MAP_BYTES as u64).map_err(storage_error)?;
        } else if length != MAP_BYTES as u64 {
            return Err("Invalid search revision file".into());
        }
        // SAFETY: fd is open read/write, offset is page aligned, and the file has a
        // full page. New files are zero-filled, a valid AtomicU64 representation.
        let pointer = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                MAP_BYTES,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if pointer == libc::MAP_FAILED {
            return Err("Search revision mapping failed".into());
        }
        let pointer =
            NonNull::new(pointer.cast::<Counters>()).ok_or("Search revision mapping failed")?;
        Ok(Self { pointer })
    }

    pub fn counters(&self) -> &Counters {
        // SAFETY: mmap provides aligned live storage for the two atomic counters;
        // lifetime is bounded by self, and all processes leave this file allocated.
        unsafe { self.pointer.as_ref() }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: this is the original mapping and length. Arc ownership ensures no
        // thread in this process can access it after the final owner is dropped.
        unsafe {
            libc::munmap(self.pointer.as_ptr().cast(), MAP_BYTES);
        }
    }
}
