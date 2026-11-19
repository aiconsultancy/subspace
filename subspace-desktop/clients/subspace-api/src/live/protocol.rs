//! Incremental UTF-8 SSE framing. Bounds include unknown fields and comments.
use super::LiveError;
use serde::Deserialize;
use std::collections::VecDeque;

const MAX_EVENT_BYTES: usize = 64 * 1024;

/// One issue hint. The visible state is read again through the REST API.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct IssueChanged {
    pub project_key: String,
    pub issue_id: String,
    pub identifier: String,
    #[serde(default)]
    pub parent_identifier: Option<String>,
    pub version: i64,
    #[serde(default)]
    pub audit_id: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub actor_type: Option<String>,
    #[serde(default)]
    pub occurred_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Record {
    Ready,
    Changed(IssueChanged),
    Other,
}

#[derive(Default)]
pub(super) struct Records {
    pub(super) ready: VecDeque<Record>,
    line: Vec<u8>,
    name: String,
    data: Vec<u8>,
    /// CR already ended the line. Consume an LF if the next chunk starts with one.
    skip_lf: bool,
    started: bool,
    size: usize,
}

impl Records {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<(), LiveError> {
        for &byte in chunk {
            if std::mem::take(&mut self.skip_lf) && byte == b'\n' {
                continue;
            }
            self.size += 1;
            if self.size > MAX_EVENT_BYTES {
                return Err(LiveError::Protocol);
            }
            if matches!(byte, b'\r' | b'\n') {
                self.skip_lf = byte == b'\r';
                self.end_line()?;
            } else {
                self.line.push(byte);
            }
        }
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), LiveError> {
        let mut line = std::str::from_utf8(&self.line).map_err(|_| LiveError::Protocol)?;
        if !std::mem::replace(&mut self.started, true) {
            line = line.trim_start_matches('\u{feff}');
        }
        if line.is_empty() {
            if let Some(record) = self.record()? {
                self.ready.push_back(record);
            }
            self.size = 0;
            self.name.clear();
            self.data.clear();
        } else {
            let (name, value) = line.split_once(':').unwrap_or((line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value);
            match name {
                "event" => {
                    self.name.clear();
                    self.name.push_str(value);
                }
                "data" => {
                    self.data.extend_from_slice(value.as_bytes());
                    self.data.push(b'\n');
                }
                _ => {}
            }
        }
        self.line.clear();
        Ok(())
    }

    fn record(&self) -> Result<Option<Record>, LiveError> {
        let Some((_, data)) = self.data.split_last() else {
            return Ok(None);
        };
        match self.name.as_str() {
            "ready" => Ok(Some(Record::Ready)),
            "issueChanged" => serde_json::from_slice(data)
                .map(|hint| Some(Record::Changed(hint)))
                .map_err(|_| LiveError::Protocol),
            _ => Ok(Some(Record::Other)),
        }
    }
}
