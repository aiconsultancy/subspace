use std::fs;
use std::io::{self, Write as _};

use super::super::{files, marker};
use super::{Fixture, generation};

#[test]
fn reset_publishes_a_different_token_after_a_reader_observes_old_and_partial_bytes() {
    let fixture = Fixture::new(100, 5);
    fixture.open().append(b"{\"old\":1}\n").unwrap();
    let before = generation(&fixture.live).unwrap();
    let mut during = None;
    let after = marker::reset(&fixture.live, || {
        during = generation(&fixture.live)?;
        assert_ne!(during, before);
        assert_eq!(fs::read(&fixture.live)?, b"{\"old\":1}\n");
        files::truncate(&fixture.live)?;
        let mut file = files::open_private(&fixture.live)?;
        file.write_all(b"{\"new\":")?;
        assert_eq!(generation(&fixture.live)?, during);
        assert_eq!(fs::read(&fixture.live)?, b"{\"new\":");
        file.write_all(b"2}\n")?;
        Ok(())
    })
    .unwrap();
    assert_ne!(Some(&after), during.as_ref());
    assert_eq!(generation(&fixture.live).unwrap(), Some(after));
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{\"new\":2}\n");
}

#[test]
fn failed_post_reset_publication_does_not_allow_a_writer_to_append() {
    let fixture = Fixture::new(100, 5);
    let mut writer = fixture.open();
    writer.append(b"{\"old\":1}\n").unwrap();
    let path = fixture.live.with_file_name("app.log.run-day-v1");
    let result = marker::reset(&fixture.live, || {
        files::truncate(&fixture.live)?;
        fs::remove_file(&path)?;
        fs::create_dir(&path)
    });
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert!(writer.append(b"{\"new\":2}\n").is_err());
    assert_eq!(fs::metadata(&fixture.live).unwrap().len(), 0);
    assert_eq!(
        fs::read(fixture.dated("2026-01-01")).unwrap(),
        b"{\"old\":1}\n"
    );
    fs::remove_dir(&path).unwrap();
    writer.append(b"{\"new\":2}\n").unwrap();
    assert_eq!(
        fs::read(&fixture.live).unwrap(),
        b"{\"old\":1}\n{\"new\":2}\n"
    );
}
