use crate::project::limits;
use anyhow::{Result, ensure};
use std::io::BufRead;

/// Reads one bounded newline-terminated frame without an unbounded read_line.
pub(super) fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            ensure!(frame.is_empty(), "unterminated worker frame");
            return Ok(None);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |ix| ix + 1);
        ensure!(
            frame.len() + consumed <= limits::FRAME_BYTES,
            "worker frame exceeds 256 KiB"
        );
        frame.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if newline.is_some() {
            frame.pop();
            return Ok(Some(frame));
        }
    }
}
