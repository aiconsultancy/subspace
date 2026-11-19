//! Bound the declared central directory before `zip` allocates its index.
//! Packages below our byte and entry limits never require ZIP64 or split disks.

use super::files::MAX_ENTRIES;

const END_MAGIC: &[u8] = b"PK\x05\x06";
const END_BYTES: usize = 22;
const MAX_COMMENT_BYTES: usize = u16::MAX as usize;
const DISK: usize = 4;
const DIRECTORY_DISK: usize = 6;
const DISK_ENTRIES: usize = 8;
const ENTRIES: usize = 10;
const DIRECTORY_BYTES: usize = 12;
const DIRECTORY_OFFSET: usize = 16;
const COMMENT_BYTES: usize = 20;

pub(super) fn check(bytes: &[u8]) -> Result<(), String> {
    let offset = end_offset(bytes)?;
    let end = &bytes[offset..];
    let entries = word(end, ENTRIES) as usize;
    let size = dword(end, DIRECTORY_BYTES) as usize;
    let start = dword(end, DIRECTORY_OFFSET) as usize;
    if word(end, DISK) != 0
        || word(end, DIRECTORY_DISK) != 0
        || word(end, DISK_ENTRIES) as usize != entries
        || entries > MAX_ENTRIES
        || start.checked_add(size) != Some(offset)
    {
        return Err(
            "Use a regular ZIP with at most 10,000 entries, without ZIP64 or split disks.".into(),
        );
    }
    Ok(())
}

fn end_offset(bytes: &[u8]) -> Result<usize, String> {
    if bytes.len() < END_BYTES {
        return Err("Cannot read this ZIP header.".into());
    }
    let first = bytes.len().saturating_sub(END_BYTES + MAX_COMMENT_BYTES);
    (first..=bytes.len() - END_BYTES)
        .rev()
        .find(|offset| {
            let end = &bytes[*offset..];
            end.starts_with(END_MAGIC)
                && *offset + END_BYTES + word(end, COMMENT_BYTES) as usize == bytes.len()
        })
        .ok_or_else(|| "Cannot read this ZIP directory.".into())
}

fn word(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
fn dword(bytes: &[u8], offset: usize) -> u32 {
    let pair = [word(bytes, offset), word(bytes, offset + size_of::<u16>())];
    u32::from(pair[0]) | (u32::from(pair[1]) << u16::BITS)
}
