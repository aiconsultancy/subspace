use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;

use super::{Limits, RunDayLog, clear, generation, retention};

mod collisions;
mod concurrency;
mod generation;
mod migration;

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

struct Fixture {
    _directory: tempfile::TempDir,
    live: PathBuf,
    today: Arc<Mutex<NaiveDate>>,
    limits: Limits,
}

impl Fixture {
    fn new(bytes: u64, days: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        Self {
            live: directory.path().join("app.log"),
            _directory: directory,
            today: Arc::new(Mutex::new(date("2026-01-01"))),
            limits: Limits::new(bytes, days),
        }
    }

    fn open(&self) -> RunDayLog {
        let today = self.today.clone();
        RunDayLog::open_with_clock(
            &self.live,
            self.limits,
            Arc::new(move || *today.lock().unwrap()),
        )
        .unwrap()
    }

    fn day(&self, day: &str) {
        *self.today.lock().unwrap() = date(day);
    }

    fn dated(&self, day: &str) -> PathBuf {
        self.live.with_file_name(format!("app.log.{day}"))
    }

    fn dates(&self) -> Vec<String> {
        retention::dates(&self.live)
            .unwrap()
            .into_iter()
            .map(|(date, _)| date.to_string())
            .collect()
    }
}

#[test]
fn sparse_run_dates_survive_restarts_and_the_sixth_evicts_only_the_oldest() {
    let fixture = Fixture::new(100, 5);
    for day in [
        "2026-01-01",
        "2026-02-08",
        "2026-04-03",
        "2026-07-12",
        "2026-09-10",
    ] {
        fixture.day(day);
        fixture.open().append(b"{\"run\":1}\n").unwrap();
    }
    assert_eq!(fixture.dates().len(), 5);
    assert!(fixture.dated("2026-01-01").exists());
    fixture.day("2026-11-09");
    fixture.open().append(b"{\"run\":2}\n").unwrap();
    assert_eq!(
        fixture.dates(),
        [
            "2026-02-08",
            "2026-04-03",
            "2026-07-12",
            "2026-09-10",
            "2026-11-09"
        ]
    );
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"run\":2}\n");
}

#[test]
fn opening_creates_no_run_day_and_same_day_reopen_appends() {
    let fixture = Fixture::new(100, 5);
    let mut writer = fixture.open();
    assert!(fixture.dates().is_empty());
    assert_eq!(writer.live_path(), fixture.live);
    writer.append(b"{\"run\":1}\n").unwrap();
    drop(writer);
    fixture.open().append(b"{\"run\":2}\n").unwrap();
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{\"run\":1}\n{\"run\":2}\n"
    );
    assert_eq!(fixture.dates(), ["2026-01-01"]);
}

#[test]
fn first_write_after_midnight_selects_a_date_and_advances_generation() {
    let fixture = Fixture::new(100, 5);
    let mut writer = fixture.open();
    writer.append(b"{\"old\":1}\n").unwrap();
    let before = generation(&fixture.live).unwrap();
    fixture.day("2026-01-07");
    writer.append(b"{\"new\":2}\n").unwrap();
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{\"old\":1}\n"
    );
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"new\":2}\n");
    assert_ne!(generation(&fixture.live).unwrap(), before);
    assert_eq!(fixture.dates(), ["2026-01-01", "2026-01-07"]);
}

#[test]
fn rollover_discards_the_previous_chunk_and_rejects_oversized_input_first() {
    let fixture = Fixture::new(20, 1);
    let mut writer = fixture.open();
    writer.append(b"{\"old\":1}\n").unwrap();
    writer.append(b"{\"fit\":2}\n").unwrap();
    let before = generation(&fixture.live).unwrap();
    writer.append(b"{\"new\":3}\n").unwrap();
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"new\":3}\n");
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{\"new\":3}\n"
    );
    assert_ne!(generation(&fixture.live).unwrap(), before);
    let before_rejection = generation(&fixture.live).unwrap();
    fixture.day("2026-02-01");
    assert_eq!(
        writer
            .append(b"{\"oversized\":\"test-only\"}\n")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fixture.dates(), ["2026-01-01"]);
    assert_eq!(generation(&fixture.live).unwrap(), before_rejection);
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"new\":3}\n");
}

#[test]
fn an_active_date_is_protected_when_the_clock_moves_backwards() {
    let fixture = Fixture::new(100, 2);
    let mut writer = fixture.open();
    for day in ["2026-05-01", "2026-06-01", "2026-01-01"] {
        fixture.day(day);
        writer.append(b"{}\n").unwrap();
    }
    assert_eq!(fixture.dates(), ["2026-01-01", "2026-06-01"]);
    writer.append(b"{\"active\":true}\n").unwrap();
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{}\n{\"active\":true}\n"
    );
}

#[test]
fn clear_removes_history_and_an_existing_writer_resumes_at_visible_paths() {
    let fixture = Fixture::new(100, 5);
    let mut writer = fixture.open();
    writer.append(b"{\"old\":1}\n").unwrap();
    fixture.day("2026-01-02");
    writer.append(b"{\"old\":2}\n").unwrap();
    let before = generation(&fixture.live).unwrap();
    clear(&fixture.live).unwrap();
    assert!(fixture.dates().is_empty());
    assert_eq!(fs::metadata(&fixture.live).unwrap().len(), 0);
    assert_ne!(generation(&fixture.live).unwrap(), before);
    writer.append(b"{\"new\":3}\n").unwrap();
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"new\":3}\n");
    assert_eq!(
        fs::read(fixture.dated("2026-01-02")).unwrap(),
        b"{\"new\":3}\n"
    );
    assert_eq!(fixture.dates(), ["2026-01-02"]);
}

#[test]
fn foreign_names_invalid_dates_and_directories_are_not_managed() {
    let fixture = Fixture::new(100, 1);
    let mut writer = fixture.open();
    let parent = fixture.live.parent().unwrap();
    for name in [
        "app.log.2026-02-30",
        "app.log.2026-1-01",
        "other.log.2020-01-01",
        "app.log.01",
        "app.log.6",
    ] {
        fs::write(parent.join(name), b"foreign").unwrap();
    }
    fs::create_dir(fixture.dated("2025-01-01")).unwrap();
    writer.append(b"{}\n").unwrap();
    clear(&fixture.live).unwrap();
    assert_eq!(fs::read(parent.join("app.log.6")).unwrap(), b"foreign");
    assert_eq!(
        fs::read(parent.join("app.log.2026-02-30")).unwrap(),
        b"foreign"
    );
    assert!(fixture.dated("2025-01-01").is_dir());
    assert_eq!(fs::read_dir(parent).unwrap().count(), 9);
}

#[test]
fn an_unwritable_marker_blocks_resets_without_unbounded_appends() {
    let fixture = Fixture::new(20, 5);
    let mut writer = fixture.open();
    writer.append(b"{}\n").unwrap();
    let marker = fixture.live.with_file_name("app.log.run-day-v1");
    fs::remove_file(&marker).unwrap();
    fs::create_dir(&marker).unwrap();
    fixture.day("2026-01-02");
    for _ in 0..10 {
        assert!(writer.append(b"{\"new\":1}\n").is_err());
    }
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{}\n");
    assert!(!fixture.dated("2026-01-02").exists());
    assert!(marker.is_dir());
    fs::remove_dir(&marker).unwrap();
    writer.append(b"{\"new\":1}\n").unwrap();
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"new\":1}\n");
}

#[test]
fn malformed_or_empty_records_do_not_create_dates_or_change_generation() {
    let fixture = Fixture::new(100, 5);
    let mut writer = fixture.open();
    let before = generation(&fixture.live).unwrap();
    for line in [b"".as_slice(), b"{}", b"{}\n{}\n"] {
        assert_eq!(
            writer.append(line).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(fixture.dates().is_empty());
    assert_eq!(generation(&fixture.live).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn owned_symlinks_are_neither_followed_nor_deleted() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new(100, 1);
    let mut writer = fixture.open();
    let target = fixture.live.with_file_name("foreign");
    fs::write(&target, b"untouched").unwrap();
    symlink(&target, fixture.dated("2025-01-01")).unwrap();
    symlink(&target, fixture.dated("2026-01-01")).unwrap();
    assert!(writer.append(b"{}\n").is_err());
    clear(&fixture.live).unwrap();
    assert!(
        fs::symlink_metadata(fixture.dated("2025-01-01"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), b"untouched");
}

#[cfg(unix)]
#[test]
fn failed_pruning_blocks_new_allocation_without_growing_live() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::new(100, 1);
    let mut writer = fixture.open();
    writer.append(b"{}\n").unwrap();
    let parent = fixture.live.parent().unwrap();
    fs::set_permissions(parent, fs::Permissions::from_mode(0o500)).unwrap();
    fixture.day("2026-01-02");
    let result = writer.append(b"{\"new\":1}\n");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(fixture.dates(), ["2026-01-01"]);
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{}\n");
}

#[cfg(unix)]
#[test]
fn created_and_reopened_files_are_owner_only_and_live_inode_is_stable() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let fixture = Fixture::new(10, 5);
    let mut writer = fixture.open();
    let inode = fs::metadata(&fixture.live).unwrap().ino();
    writer.append(b"{}\n").unwrap();
    writer.append(b"{\"x\":1}\n").unwrap();
    assert_eq!(fs::metadata(&fixture.live).unwrap().ino(), inode);
    for name in ["app.log", "app.log.2026-01-01", "app.log.run-day-v1"] {
        assert_eq!(
            fs::metadata(fixture.live.with_file_name(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
