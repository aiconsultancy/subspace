//! Frames IRC lines and applies the independent outbound tag and body limits.

use crate::proto::{Message, serialise};

pub const BODY_LIMIT: usize = 512;
pub const SERVER_TAGS_LIMIT: usize = 8191;
pub const CLIENT_TAGS_LIMIT: usize = 4096;

#[derive(Debug, Default)]
pub struct LineBuffer {
    bytes: Vec<u8>,
    consumed: usize,
}

#[derive(Debug, thiserror::Error)]
#[error("The server sent an unterminated IRC line exceeding 8703 bytes")]
pub struct LineTooLong;

impl LineBuffer {
    pub fn extend(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn next_line(&mut self) -> Result<Option<String>, LineTooLong> {
        let pending = &self.bytes[self.consumed..];
        if let Some(end) = pending.iter().position(|&byte| byte == b'\n') {
            let line = String::from_utf8_lossy(&pending[..end]);
            let line = line.trim_end_matches('\r').to_owned();
            self.consumed += end + 1;
            return Ok(Some(line));
        }
        if pending.len() > SERVER_TAGS_LIMIT + BODY_LIMIT {
            return Err(LineTooLong);
        }
        self.bytes.drain(..self.consumed);
        self.consumed = 0;
        Ok(None)
    }
}

pub fn encode(message: &Message) -> Vec<u8> {
    let wire = serialise(message);
    let (tags, body) = if wire.starts_with('@') {
        wire.split_once(' ')
            .map_or((None, wire.as_str()), |(tags, body)| (Some(tags), body))
    } else {
        (None, wire.as_str())
    };
    let mut bytes = Vec::new();
    if let Some(tags) = tags {
        bytes
            .extend_from_slice(&tags.as_bytes()[..tags.floor_char_boundary(CLIENT_TAGS_LIMIT - 1)]);
        bytes.push(b' ');
    }
    bytes.extend_from_slice(&body.as_bytes()[..body.floor_char_boundary(BODY_LIMIT - 2)]);
    bytes.extend_from_slice(b"\r\n");
    bytes
}
