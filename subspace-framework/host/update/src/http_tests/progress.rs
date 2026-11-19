use std::{cell::RefCell, io::Write, thread};

use crate::UpdateProgress;

use super::{
    NEWEST, Route, archive, checksum_line, installed_executable, local_api, plain_asset,
    read_request, release_route, release_with_assets, serve, updater_against,
};

fn download_samples(events: &[UpdateProgress]) -> Vec<(u64, Option<u64>)> {
    events
        .iter()
        .filter_map(|event| match event {
            UpdateProgress::Downloading { received, total } => Some((*received, *total)),
            _ => None,
        })
        .collect()
}

fn download_fixture(base: &str) -> (Vec<u8>, Vec<UpdateProgress>) {
    let updater = updater_against(base);
    let events = RefCell::new(Vec::new());
    let bytes = updater
        .client()
        .unwrap()
        .asset_with_progress(&release_with_assets(base, "fixture"), "fixture", &|event| {
            events.borrow_mut().push(event)
        })
        .unwrap();
    (bytes, events.into_inner())
}

#[test]
fn progress_reads_stop_at_the_limit_plus_one_without_accepting_oversized_data() {
    let mut reader = std::io::Cursor::new(vec![42; 1024]);
    let received = RefCell::new(Vec::new());
    let result = crate::archive::read_limited_with_progress(&mut reader, 8, &|bytes| {
        received.borrow_mut().push(bytes);
    });
    assert!(result.unwrap_err().to_string().contains("exceeds 8 bytes"));
    assert_eq!(reader.position(), 9);
    assert!(received.into_inner().iter().all(|bytes| *bytes <= 8));
}

fn assert_download(events: &[UpdateProgress], length: u64, total: Option<u64>) {
    let samples = download_samples(events);
    assert_eq!(samples.first(), Some(&(0, total)));
    assert_eq!(samples.last(), Some(&(length, total)));
    assert!(samples.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert!(
        samples
            .iter()
            .all(|(received, size)| *received <= length && *size == total)
    );
}

#[test]
fn progress_reports_plain_install_phases_in_order() {
    let (listener, base) = local_api();
    let (asset, zip) = plain_asset();
    let body = archive(zip, NEWEST);
    let length = body.len() as u64;
    let server = serve(
        listener,
        vec![
            release_route(&base, &asset),
            Route::new("GET /assets/1 ", body.clone()),
            Route::new("GET /assets/2 ", checksum_line(&asset, &body)),
        ],
        3,
    );
    let directory = tempfile::tempdir().unwrap();
    let executable = installed_executable(directory.path(), zip);
    let events = RefCell::new(Vec::new());
    updater_against(&base)
        .install_at_with_progress(&release_with_assets(&base, &asset), &executable, &|event| {
            assert_eq!(std::fs::read(&executable).unwrap(), b"old binary");
            events.borrow_mut().push(event);
        })
        .unwrap();
    server.join().unwrap();
    let events = events.into_inner();
    assert_download(&events, length, Some(length));
    let mut phases = events;
    phases.dedup_by(|later, earlier| {
        matches!(later, UpdateProgress::Downloading { .. })
            && matches!(earlier, UpdateProgress::Downloading { .. })
    });
    assert_eq!(
        phases,
        vec![
            UpdateProgress::Preparing,
            UpdateProgress::Downloading {
                received: 0,
                total: Some(length)
            },
            UpdateProgress::DownloadingChecksum,
            UpdateProgress::VerifyingChecksum,
            UpdateProgress::Extracting,
            UpdateProgress::Installing,
        ]
    );
    assert_ne!(std::fs::read(executable).unwrap(), b"old binary");
}

#[test]
fn failed_checksum_progress_stops_before_extraction_and_installation() {
    let (listener, base) = local_api();
    let (asset, zip) = plain_asset();
    let server = serve(
        listener,
        vec![
            release_route(&base, &asset),
            Route::new("GET /assets/1 ", archive(zip, NEWEST)),
            Route::new("GET /assets/2 ", checksum_line(&asset, b"wrong")),
        ],
        3,
    );
    let directory = tempfile::tempdir().unwrap();
    let executable = installed_executable(directory.path(), zip);
    let events = RefCell::new(Vec::new());
    let result = updater_against(&base).install_at_with_progress(
        &release_with_assets(&base, &asset),
        &executable,
        &|event| events.borrow_mut().push(event),
    );
    server.join().unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch")
    );
    let events = events.into_inner();
    assert_eq!(events.last(), Some(&UpdateProgress::VerifyingChecksum));
    assert!(!events.iter().any(|event| matches!(
        event,
        UpdateProgress::Extracting
            | UpdateProgress::VerifyingSignature
            | UpdateProgress::Installing
    )));
    assert_eq!(std::fs::read(executable).unwrap(), b"old binary");
}

#[test]
fn download_progress_reports_multiple_actual_byte_counts_and_known_total() {
    let (listener, base) = local_api();
    let body = vec![42; 256 * 1024];
    let length = body.len() as u64;
    let server = serve(
        listener,
        vec![Route::new("GET /assets/1 ", body.clone())],
        1,
    );
    let (bytes, events) = download_fixture(&base);
    server.join().unwrap();
    assert_eq!(bytes, body);
    assert_download(&events, length, Some(length));
    assert!(
        download_samples(&events).len() > 2,
        "bounded reads must report intermediate bytes"
    );
}

#[test]
fn download_progress_keeps_unknown_length_indeterminate() {
    let (listener, base) = local_api();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let request = read_request(&mut socket);
        assert!(request.contains("/assets/1"));
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nunknown length")
            .unwrap();
    });
    let (bytes, events) = download_fixture(&base);
    server.join().unwrap();
    assert_eq!(bytes, b"unknown length");
    assert_download(&events, bytes.len() as u64, None);
}
