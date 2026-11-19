//! Small persistent migration version and opaque live-chunk generation.

use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::path::Path;

use super::files;

const VERSION: &str = "run-day-v1\n";
const MAX_MARKER_READ_BYTES: u64 = 129;
const TOKEN_HEX_LEN: usize = 32;

/// Returns the comparable live-chunk token, or `None` before a marker exists.
/// Readers treat a token change as a reset even if the live file has regrown.
pub fn generation(live_path: &Path) -> io::Result<Option<String>> {
    let path = files::suffixed(live_path, ".run-day-v1");
    files::open_optional(&path)?.map(read_marker).transpose()
}

fn read_marker(file: File) -> io::Result<String> {
    let mut text = String::new();
    file.take(MAX_MARKER_READ_BYTES).read_to_string(&mut text)?;
    parse_generation(&text)
}

fn parse_generation(text: &str) -> io::Result<String> {
    let token = text
        .strip_prefix(VERSION)
        .and_then(|text| text.strip_suffix('\n'));
    match token {
        Some(token)
            if token.len() == TOKEN_HEX_LEN
                && token.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
        {
            Ok(token.to_owned())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid log generation marker",
        )),
    }
}

pub(super) fn exists(live: &Path) -> io::Result<bool> {
    files::regular(&files::suffixed(live, ".run-day-v1")).map(|entry| entry.is_some())
}

pub(super) fn advance(live: &Path) -> io::Result<String> {
    let path = files::suffixed(live, ".run-day-v1");
    files::regular(&path)?;
    let token = format!("{:032x}", rand::random::<u128>());
    let mut temp = tempfile::NamedTempFile::new_in(files::directory(live)?)?;
    crate::private_file::set_owner_only(temp.path())?;
    temp.write_all(format!("{VERSION}{token}\n").as_bytes())?;
    temp.flush()?;
    temp.persist(&path).map_err(|error| error.error)?;
    Ok(token)
}

/// Bracket a reset so a reader that consumes the pre-token with old or partial
/// contents must observe a different token after the completed reset.
pub(super) fn reset(
    live: &Path,
    reset_contents: impl FnOnce() -> io::Result<()>,
) -> io::Result<String> {
    advance(live)?;
    reset_contents()?;
    advance(live)
}
