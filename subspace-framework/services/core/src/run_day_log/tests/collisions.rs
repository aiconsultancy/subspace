use std::fs;

use super::{Fixture, RunDayLog, generation};

fn assert_rejected_without_eviction(fixture: &Fixture, writer: &mut RunDayLog) {
    let before = generation(&fixture.live).unwrap();
    fixture.day("2026-01-02");
    assert!(writer.append(b"{\"new\":2}\n").is_err());
    assert!(
        fixture.dated("2026-01-01").is_file(),
        "a rejected date must not evict recorded history"
    );
    assert_eq!(fixture.dates(), ["2026-01-01"]);
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{\"old\":1}\n"
    );
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"old\":1}\n");
    assert_eq!(generation(&fixture.live).unwrap(), before);
}

#[test]
fn a_dated_directory_collision_is_rejected_before_pruning_history() {
    let fixture = Fixture::new(100, 1);
    let mut writer = fixture.open();
    writer.append(b"{\"old\":1}\n").unwrap();
    let collision = fixture.dated("2026-01-02");
    fs::create_dir(&collision).unwrap();
    fs::write(collision.join("foreign"), b"untouched").unwrap();
    assert_rejected_without_eviction(&fixture, &mut writer);
    assert!(collision.is_dir());
    assert_eq!(fs::read(collision.join("foreign")).unwrap(), b"untouched");
}

#[cfg(unix)]
#[test]
fn a_dated_symlink_collision_is_rejected_before_pruning_history() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new(100, 1);
    let mut writer = fixture.open();
    writer.append(b"{\"old\":1}\n").unwrap();
    let target = fixture.live.with_file_name("foreign");
    fs::write(&target, b"untouched").unwrap();
    let collision = fixture.dated("2026-01-02");
    symlink(&target, &collision).unwrap();
    assert_rejected_without_eviction(&fixture, &mut writer);
    assert!(
        fs::symlink_metadata(&collision)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), b"untouched");
}
