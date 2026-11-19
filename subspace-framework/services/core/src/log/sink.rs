//! Where records go: the ring buffer the viewer reads, and the bounded run-day
//! JSON Lines file once [`init`] has opened one.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;

use crate::run_day_log::{Limits, RunDayLog};

use super::{BUFFER_RECORDS, LEVEL_ENV, Level, Record};

/// The default size a log file grows to before it rotates (5 MiB).
pub const DEFAULT_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
/// How many local dates with writes are kept, including the active date.
pub const DEFAULT_MAX_FILES: usize = 5;

/// Where and how much to log.
#[derive(Debug, Clone)]
pub struct Config {
    pub directory: PathBuf,
    /// `subspace` gives `subspace.log` and `subspace.log.YYYY-MM-DD`.
    pub file_stem: String,
    pub level: Level,
    pub max_file_bytes: u64,
    pub max_files: usize,
}

impl Config {
    pub fn new(directory: PathBuf, file_stem: impl Into<String>, level: Level) -> Self {
        Self {
            directory,
            file_stem: file_stem.into(),
            level,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_files: DEFAULT_MAX_FILES,
        }
    }
}

struct Writer {
    config: Config,
    log: RunDayLog,
}

#[derive(Default)]
struct Sink {
    next_seq: u64,
    buffer: VecDeque<Record>,
    writer: Option<Writer>,
}

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();
/// The level as a `u8` index into [`Level::ALL`], read on every call.
static LEVEL: AtomicU8 = AtomicU8::new(2);

fn sink() -> &'static Mutex<Sink> {
    SINK.get_or_init(Mutex::default)
}

/// The level records must reach to be kept.
pub fn level() -> Level {
    let index = usize::from(LEVEL.load(Ordering::Relaxed));
    Level::ALL.get(index).copied().unwrap_or(Level::Info)
}

pub fn set_level(level: Level) {
    LEVEL.store(level.index() as u8, Ordering::Relaxed);
}

/// Opens the log file and starts writing records to it. `SUBSPACE_LOG` in
/// the environment overrides `config.level`.
pub fn init(config: Config) -> Result<(), String> {
    let level = std::env::var(LEVEL_ENV)
        .ok()
        .and_then(|value| Level::parse(&value))
        .unwrap_or(config.level);
    set_level(level);
    let path = live_path(&config);
    let log = RunDayLog::open(&path, Limits::new(config.max_file_bytes, config.max_files))
        .map_err(|error| open_error(&path, &error))?;
    let mut sink = sink()
        .lock()
        .map_err(|_| "the log sink is poisoned".to_owned())?;
    sink.writer = Some(Writer { config, log });
    Ok(())
}

fn open_error(path: &Path, error: &std::io::Error) -> String {
    format!("Cannot open the log file {}: {error}", path.display())
}

/// The directory the log file is in, once [`init`] ran.
pub fn directory() -> Option<PathBuf> {
    let sink = sink().lock().ok()?;
    Some(sink.writer.as_ref()?.config.directory.clone())
}

/// The actual live path of the installed file writer, not a default location.
pub fn path() -> Option<PathBuf> {
    let sink = sink().lock().ok()?;
    Some(sink.writer.as_ref()?.log.live_path().to_path_buf())
}

/// The buffered records, oldest first.
pub fn recent() -> Vec<Record> {
    sink()
        .lock()
        .map(|sink| sink.buffer.iter().cloned().collect())
        .unwrap_or_default()
}

/// The buffered records after sequence `seq`, oldest first.
pub fn recent_after(seq: u64) -> Vec<Record> {
    sink()
        .lock()
        .map(|sink| {
            sink.buffer
                .iter()
                .filter(|record| record.seq > seq)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn emit(level: Level, target: String, message: String, fields: Vec<(String, String)>) {
    let Ok(mut sink) = sink().lock() else {
        return;
    };
    sink.next_seq += 1;
    let record = Record {
        seq: sink.next_seq,
        at: Utc::now(),
        level,
        target,
        message,
        fields,
    };
    if let Some(writer) = sink.writer.as_mut() {
        write(writer, &record);
    }
    sink.buffer.push_back(record);
    while sink.buffer.len() > BUFFER_RECORDS {
        sink.buffer.pop_front();
    }
}

/// One JSON line, rolling over first when the date's chunk is full. A write that fails
/// is dropped: logging must never take the app down.
fn write(writer: &mut Writer, record: &Record) {
    let Ok(mut line) = serde_json::to_string(record) else {
        return;
    };
    line.push('\n');
    _ = writer.log.append(line.as_bytes());
}

fn live_path(config: &Config) -> PathBuf {
    config.directory.join(format!("{}.log", config.file_stem))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{Config, Writer, live_path, write};
    use crate::log::{Level, Record};
    use crate::run_day_log::{Limits, RunDayLog};

    fn record(seq: u64, message: &str) -> Record {
        Record {
            seq,
            at: chrono::Utc::now(),
            level: Level::Info,
            target: "test".into(),
            message: message.into(),
            fields: Vec::new(),
        }
    }

    #[test]
    fn a_full_file_rolls_over_without_numbered_archives() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            max_file_bytes: 150,
            max_files: 2,
            ..Config::new(dir.path().to_owned(), "app", Level::Info)
        };
        let mut writer = Writer {
            config: config.clone(),
            log: RunDayLog::open(&live_path(&config), Limits::new(150, 2)).unwrap(),
        };
        for seq in 1..=12 {
            write(&mut writer, &record(seq, "a message that is long enough"));
        }
        assert!(live_path(&config).exists());
        assert!(!dir.path().join("app.log.1").exists());
        let live = fs::read_to_string(live_path(&config)).unwrap();
        assert!(live.len() <= 150, "the live file stays under the limit");
        let last = live.lines().last().unwrap();
        let parsed: Record = serde_json::from_str(last).unwrap();
        assert_eq!(parsed.seq, 12);
    }

    #[test]
    fn an_oversized_record_does_not_exceed_the_disk_limit() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            max_file_bytes: 150,
            ..Config::new(dir.path().to_owned(), "app", Level::Info)
        };
        let mut writer = Writer {
            config: config.clone(),
            log: RunDayLog::open(&live_path(&config), Limits::new(150, 5)).unwrap(),
        };
        write(&mut writer, &record(1, &"x".repeat(200)));
        assert_eq!(fs::metadata(live_path(&config)).unwrap().len(), 0);
    }

    #[test]
    fn path_reports_the_installed_writer_and_disk_failures_leave_the_ring_working() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path().to_owned(), "selected-product", Level::Info);
        let actual = live_path(&config);
        let log = RunDayLog::open(&actual, Limits::new(1, 5)).unwrap();
        let previous = super::sink()
            .lock()
            .unwrap()
            .writer
            .replace(Writer { config, log });
        assert_eq!(super::path(), Some(actual.clone()));
        super::emit(
            Level::Info,
            "disk-failure-fixture".into(),
            "retained in memory".into(),
            Vec::new(),
        );
        assert!(
            super::recent()
                .iter()
                .any(|record| record.target == "disk-failure-fixture")
        );
        assert_eq!(fs::metadata(&actual).unwrap().len(), 0);
        super::sink().lock().unwrap().writer = None;
        assert_eq!(super::path(), None);
        super::sink().lock().unwrap().writer = previous;
    }

    #[test]
    fn the_level_round_trips_through_the_atomic() {
        super::set_level(Level::Trace);
        assert_eq!(super::level(), Level::Trace);
        super::set_level(Level::Info);
        assert_eq!(super::level(), Level::Info);
    }
}
