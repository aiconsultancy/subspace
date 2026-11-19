use std::fs;
use std::path::Path;

use chrono::{Local, NaiveDateTime, TimeZone as _};

use super::{Fixture, generation, retention};

fn legacy(path: &Path, time: &str, text: &[u8]) {
    fs::write(path, text).unwrap();
    let time = NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S").unwrap();
    let time = Local.from_local_datetime(&time).single().unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_times(fs::FileTimes::new().set_modified(time.into()))
        .unwrap();
}

#[test]
fn legacy_numbered_and_live_logs_migrate_once_by_modification_date() {
    let fixture = Fixture::new(20, 5);
    let parent = fixture.live.parent().unwrap();
    legacy(
        &parent.join("app.log.1"),
        "2026-01-01 12:00:00",
        b"{}\n{\"old\":1}\nnot-json\n{\"partial\":",
    );
    legacy(&fixture.live, "2026-02-01 12:00:00", b"{\"live\":1}\n");
    let mut writer = fixture.open();
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{}\n{\"old\":1}\n"
    );
    assert_eq!(
        fs::read(fixture.dated("2026-02-01")).unwrap(),
        b"{\"live\":1}\n"
    );
    assert!(!parent.join("app.log.1").exists());
    fixture.day("2026-03-01");
    writer.append(b"{}\n").unwrap();
    drop(writer);
    legacy(
        &parent.join("app.log.1"),
        "2026-04-01 12:00:00",
        b"{\"later\":1}\n",
    );
    fixture.open();
    assert!(!fixture.dated("2026-04-01").exists());
    assert!(parent.join("app.log.1").exists());
}

#[test]
fn migration_selects_five_dates_before_import_and_bounds_legacy_chunks() {
    let fixture = Fixture::new(10, 5);
    for (index, time) in [
        (5, "2026-01-01 12:00:00"),
        (4, "2026-02-01 12:00:00"),
        (3, "2026-03-01 12:00:00"),
        (2, "2026-04-01 12:00:00"),
        (1, "2026-05-01 12:00:00"),
    ] {
        let path = fixture.live.with_file_name(format!("app.log.{index}"));
        legacy(
            &path,
            time,
            b"{}\n{}\n{}\n{}\n{\"oversized\":\"test-only\"}\n{\"z\":1}\n",
        );
    }
    legacy(&fixture.live, "2026-06-01 12:00:00", b"{}\n");
    fixture.open();
    assert_eq!(
        fixture.dates(),
        [
            "2026-02-01",
            "2026-03-01",
            "2026-04-01",
            "2026-05-01",
            "2026-06-01"
        ]
    );
    assert_eq!(
        fs::read(fixture.dated("2026-05-01")).unwrap(),
        b"{\"z\":1}\n"
    );
    let total: u64 = retention::dates(&fixture.live)
        .unwrap()
        .iter()
        .map(|(_, path)| fs::metadata(path).unwrap().len())
        .sum();
    assert!(total + fs::metadata(&fixture.live).unwrap().len() <= 60);
    assert!(!fixture.live.with_file_name("app.log.5").exists());
}

#[test]
fn missing_markers_retry_without_duplicating_existing_canonical_records() {
    let fixture = Fixture::new(100, 5);
    let source = fixture.live.with_file_name("app.log.1");
    legacy(&source, "2026-01-01 12:00:00", b"{\"imported\":1}\n");
    fixture.open();
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{\"imported\":1}\n"
    );
    fs::remove_file(fixture.live.with_file_name("app.log.run-day-v1")).unwrap();
    legacy(&source, "2026-01-01 12:00:00", b"{\"imported\":1}\n");
    fixture.open();
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{\"imported\":1}\n"
    );
    assert!(
        source.exists(),
        "unimported duplicate legacy data remains separate"
    );
}

#[test]
fn corrupt_regular_metadata_is_repaired_without_remigrating_legacy() {
    let fixture = Fixture::new(100, 5);
    fixture.open().append(b"{}\n").unwrap();
    fs::write(
        fixture.live.with_file_name("app.log.run-day-v1"),
        b"partial marker",
    )
    .unwrap();
    let source = fixture.live.with_file_name("app.log.1");
    legacy(&source, "2026-02-01 12:00:00", b"{\"legacy\":1}\n");
    assert!(generation(&fixture.live).is_err());
    fixture.open();
    assert!(generation(&fixture.live).unwrap().is_some());
    assert!(!fixture.dated("2026-02-01").exists());
    assert!(source.exists());
}

#[test]
fn legacy_sources_without_complete_records_do_not_evict_real_run_dates() {
    let fixture = Fixture::new(100, 2);
    fs::write(fixture.dated("2026-01-01"), b"{\"old\":1}\n").unwrap();
    fs::write(fixture.dated("2026-02-01"), b"{\"old\":2}\n").unwrap();
    legacy(
        &fixture.live.with_file_name("app.log.1"),
        "2026-03-01 12:00:00",
        b"not-json\n{\"partial\":",
    );
    fixture.open();
    assert_eq!(fixture.dates(), ["2026-01-01", "2026-02-01"]);
    assert!(fixture.live.with_file_name("app.log.1").exists());
}
