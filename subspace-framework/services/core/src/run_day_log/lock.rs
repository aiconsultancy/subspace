//! Serialize separately opened writers and processes with the standard OS lock.

use std::fs::File;
use std::io;
use std::path::Path;

use super::files;

pub(super) fn acquire(live: &Path) -> io::Result<File> {
    let path = files::suffixed(live, ".run-day-v1.lock");
    let file = files::open_private(&path)?;
    file.lock()?;
    Ok(file)
}
