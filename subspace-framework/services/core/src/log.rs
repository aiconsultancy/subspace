//! Structured logging for every app built from the Framework: a level, a
//! target (a board id, `http`, `app`), a message and named fields. Records
//! go to a rotating JSON Lines file under the state directory and to an
//! in-memory ring buffer the log viewer reads. Every message and field value
//! passes through [`redact`] first, so a token or an `Authorization` header
//! never reaches a file.
//!
//! Code builds its handle with [`Logger::new`], a board with its id as the
//! target. Before [`init`] runs, records
//! stay in the buffer, so tests never touch the file system.

mod redact;
mod sink;

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use redact::redact;
pub use sink::{Config, directory, init, level, path, recent, recent_after, set_level};

/// How many records the ring buffer keeps for the viewer.
pub const BUFFER_RECORDS: usize = 5_000;
/// The environment variable that overrides the stored level at startup.
pub const LEVEL_ENV: &str = "SUBSPACE_LOG";

/// How much a record matters, most important first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// The levels' names, in [`Level::ALL`] order.
const NAMES: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

impl Level {
    pub const ALL: [Self; 5] = [
        Self::Error,
        Self::Warn,
        Self::Info,
        Self::Debug,
        Self::Trace,
    ];

    /// The level's position in [`Level::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn as_str(self) -> &'static str {
        NAMES[self.index()]
    }

    /// The level named `text`, in any case; `warning` reads as `warn`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|level| level.as_str() == text || (text == "warning" && *level == Self::Warn))
    }

    /// Whether a record at `self` passes a filter set to `threshold`.
    pub fn passes(self, threshold: Self) -> bool {
        self <= threshold
    }
}

/// One log line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Position in this process's log, from 1.
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub level: Level,
    pub target: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<(String, String)>,
}

impl Record {
    /// `12:34:56.789 WARN  http  GET host /path 429 12ms  status=429`.
    pub fn line(&self) -> String {
        let mut line = format!(
            "{} {:<5} {}  {}",
            self.at.format("%H:%M:%S%.3f"),
            self.level.as_str().to_ascii_uppercase(),
            self.target,
            self.message
        );
        for (key, value) in &self.fields {
            line.push_str(&format!("  {key}={value}"));
        }
        line
    }
}

/// A handle that writes records for one target.
#[derive(Debug, Clone)]
pub struct Logger {
    target: Arc<str>,
}

impl Logger {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: Arc::from(target.into()),
        }
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn error(&self, message: impl Into<String>) {
        self.record(Level::Error, message, &[]);
    }

    pub fn warn(&self, message: impl Into<String>) {
        self.record(Level::Warn, message, &[]);
    }

    pub fn info(&self, message: impl Into<String>) {
        self.record(Level::Info, message, &[]);
    }

    pub fn debug(&self, message: impl Into<String>) {
        self.record(Level::Debug, message, &[]);
    }

    // qual:api
    pub fn trace(&self, message: impl Into<String>) {
        self.record(Level::Trace, message, &[]);
    }

    /// A record with named fields. Values are redacted like the message.
    pub fn record(&self, level: Level, message: impl Into<String>, fields: &[(&str, &str)]) {
        if !level.passes(sink::level()) {
            return;
        }
        let fields = fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), redact(value)))
            .collect();
        sink::emit(
            level,
            self.target.to_string(),
            redact(&message.into()),
            fields,
        );
    }
}

/// `logs` in the app's data directory, beside the state file: the default
/// log directory.
pub fn default_directory() -> Option<PathBuf> {
    crate::state::sibling("logs")
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::{Level, Logger, Record};

    #[test]
    fn levels_order_from_error_to_trace_and_parse_their_names() {
        assert!(Level::Error < Level::Trace);
        assert!(Level::Warn.passes(Level::Info));
        assert!(!Level::Debug.passes(Level::Info));
        assert_eq!(Level::parse(" WARNING "), Some(Level::Warn));
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("loud"), None);
        for level in Level::ALL {
            assert_eq!(Level::parse(level.as_str()), Some(level));
        }
    }

    #[test]
    fn a_record_line_carries_the_time_level_target_message_and_fields() {
        let record = Record {
            seq: 1,
            at: Utc.with_ymd_and_hms(2026, 10, 18, 7, 8, 9).unwrap(),
            level: Level::Warn,
            target: "http".into(),
            message: "GET host /path".into(),
            fields: vec![("status".into(), "429".into())],
        };
        assert_eq!(
            record.line(),
            "07:08:09.000 WARN  http  GET host /path  status=429"
        );
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"level\":\"warn\""), "{json}");
        assert_eq!(serde_json::from_str::<Record>(&json).unwrap(), record);
    }

    #[test]
    fn a_logger_records_its_target_and_redacts_what_it_records() {
        let logger = Logger::new("github.cache");
        assert_eq!(logger.target(), "github.cache");
        logger.record(
            Level::Error,
            // Fake credentials, split so secret scanners do not flag the source.
            concat!("Authorization: Bear", "er abc.def.ghi failed"),
            &[(
                "jwt",
                concat!(
                    "eyJhbGciOiJSUzI1NiIs",
                    ".eyJzdWIiOiIxMjM0NTY3ODkwIn0",
                    ".c2lnbmF0dXJl"
                ),
            )],
        );
        let last = super::recent()
            .into_iter()
            .rev()
            .find(|record| record.target == "github.cache")
            .unwrap();
        assert!(!last.message.contains("abc.def.ghi"), "{}", last.message);
        assert!(!last.fields[0].1.contains("eyJ"), "{}", last.fields[0].1);
    }
}
