use std::fs;
use std::sync::{Arc, Barrier};

use super::Fixture;

#[test]
fn separately_opened_writers_serialize_rollover_and_keep_the_same_live_chunk() {
    let fixture = Fixture::new(64, 5);
    let first = fixture.open();
    let second = fixture.open();
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = Vec::new();
    for (worker, mut writer) in [first, second].into_iter().enumerate() {
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            for record in 0..100 {
                let line = format!("{{\"worker\":{worker},\"record\":{record}}}\n");
                writer.append(line.as_bytes()).unwrap();
            }
        }));
    }
    let results: Vec<_> = threads.into_iter().map(|thread| thread.join()).collect();
    assert!(results.into_iter().all(|result| result.is_ok()));
    let live = fs::read(&fixture.live).unwrap();
    assert_eq!(live, fs::read(fixture.dated("2026-01-01")).unwrap());
    assert!(live.len() <= 64);
    assert!(!live.is_empty());
    for line in live.split_inclusive(|byte| *byte == b'\n') {
        assert!(
            serde_json::from_slice::<serde_json::Value>(line)
                .unwrap()
                .is_object()
        );
    }
}

#[test]
fn a_separate_handle_cannot_append_while_an_os_lock_is_held() {
    use super::super::{files, lock};
    use std::fs::TryLockError;
    use std::sync::mpsc;

    let fixture = Fixture::new(100, 5);
    let mut writer = fixture.open();
    let held = lock::acquire(&fixture.live).unwrap();
    let separate =
        files::open_private(&fixture.live.with_file_name("app.log.run-day-v1.lock")).unwrap();
    assert!(matches!(separate.try_lock(), Err(TryLockError::WouldBlock)));
    let (started, ready) = mpsc::channel();
    let (finished, result) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        started.send(()).unwrap();
        finished.send(writer.append(b"{}\n")).unwrap();
    });
    ready.recv().unwrap();
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    drop(held);
    assert!(result.recv().unwrap().is_ok());
    thread.join().unwrap();
    assert_eq!(fs::read(&fixture.live).unwrap(), b"{}\n");
}
