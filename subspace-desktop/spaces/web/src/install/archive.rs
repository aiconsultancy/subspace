use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

use super::{
    files::{Budget, MAX_ARCHIVE_BYTES, MAX_ENTRIES},
    paths::{self, Names},
};

const UNIX_KIND_MASK: u32 = 0o170000;
const UNIX_DIRECTORY: u32 = 0o040000;
const UNIX_FILE: u32 = 0o100000;

pub(super) fn extract(bytes: &[u8], destination: &Path) -> Result<(), String> {
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err("ZIP exceeds the 64 MiB download limit.".into());
    }
    super::zip_header::check(bytes)?;
    let config = zip::read::Config {
        archive_offset: zip::read::ArchiveOffset::Known(0),
    };
    let mut archive = zip::ZipArchive::with_config(config, Cursor::new(bytes))
        .map_err(|_| "Cannot read this ZIP.")?;
    if archive.len() > MAX_ENTRIES || archive.has_duplicate_names() {
        return Err("ZIP contains duplicate names or more than 10,000 entries.".into());
    }
    let mut names = Names::default();
    let mut budget = Budget::default();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "Cannot read a ZIP entry.")?;
        let directory = entry.is_dir();
        check_kind(entry.unix_mode(), directory)?;
        let name = entry
            .name()
            .map_err(|_| "ZIP filename is not valid text.")?;
        let text = name.strip_suffix('/').unwrap_or(&name).to_owned();
        let path = paths::relative(&text)?;
        names.add(&text, directory)?;
        let output = destination.join(path);
        if directory {
            std::fs::create_dir_all(output).map_err(|_| "Cannot create package directory.")?;
        } else {
            copy_entry(&mut entry, &output, &mut budget)?;
        }
    }
    Ok(())
}

fn check_kind(mode: Option<u32>, directory: bool) -> Result<(), String> {
    let kind = mode.unwrap_or_default() & UNIX_KIND_MASK;
    let expected = if directory { UNIX_DIRECTORY } else { UNIX_FILE };
    if kind != 0 && kind != expected {
        return Err("ZIP links and special files are not supported.".into());
    }
    Ok(())
}

pub(super) fn copy_entry(
    input: &mut impl Read,
    path: &Path,
    budget: &mut Budget,
) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid package path.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Cannot create package directory.")?;
    let mut output = File::create_new(path).map_err(|_| "Cannot create package file.")?;
    budget.copy(input, &mut output)?;
    output
        .sync_all()
        .map_err(|_| "Cannot finish package file.".to_owned())
}
