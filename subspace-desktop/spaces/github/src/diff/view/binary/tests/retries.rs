use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_fleet::{Error, diff::DiffSide};
use subspace_framework::client_test_support::Mock;

use super::{PATH, assert_page, binary_read_count, binary_response, click, mount};
use crate::diff::view::binary::{
    preview::{PreviewError, PreviewRead},
    read::Blob,
};
use crate::diff::view::{DiffSource, crew_tests::ReadGate};

#[gpui_kit::test]
fn a_live_change_cancels_a_preview_between_pages(cx: &mut TestAppContext) {
    let mock = Mock::new(binary_response);
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    let (source, client, mode, read) = view.read_with(window, |view, cx| {
        let DiffSource::Fleet(source) = &view.source else {
            panic!("expected Fleet diff")
        };
        (
            source.clone(),
            source.client(cx).unwrap(),
            view.fleet.mode,
            view.binary.as_ref().unwrap().closed.clone(),
        )
    });
    let (gate, waiting, release) = ReadGate::new();
    let worker = std::thread::spawn(move || {
        let blob = Blob {
            path: PATH,
            at: "B",
            side: DiffSide::New,
        };
        PreviewRead::default().read(|position| {
            let page = blob
                .read(&client, source.request(mode), position, &read)
                .map_err(PreviewError::Fleet)?;
            if position.offset == 0 {
                gate.pause();
            }
            Ok(page)
        })
    });
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    view.update(window, |view, _| {
        assert!(view.observe_crew_fingerprint(Some("C".into())))
    });
    release.send(()).unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Err(PreviewError::Fleet(Error::DiffReadCancelled))
    ));
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.contains("&page=png-tail"))
    );
    assert_page(&view, window, DiffSide::New, 0);
}

#[gpui_kit::test]
fn retry_page_resumes_a_busy_preview_without_fetching_its_prefix_again(cx: &mut TestAppContext) {
    let mock = busy_tail("0");
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    click(window, "crew-binary-preview");
    assert_page(&view, window, DiffSide::New, 0);
    click(window, "crew-binary-retry-page");
    view.read_with(window, |view, _| {
        let binary = view.binary.as_ref().unwrap();
        assert!(binary.preview.is_some());
        assert!(binary.retry.is_none());
    });
    assert_tail_was_retried(&mock, 6);
}

#[gpui_kit::test]
fn binary_retry_after_ten_keeps_the_current_bytes_until_due(cx: &mut TestAppContext) {
    let mock = busy_tail("10");
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    click(window, "crew-binary-next");
    assert_page(&view, window, DiffSide::New, 0);
    window.update(|window, _| {
        let next = window.find("crew-binary-next");
        assert!(next.visible());
        assert_eq!(next.disabled(), Some(true));
        assert_eq!(window.find("crew-binary-retry-page").disabled(), Some(true));
    });
    window.executor().advance_clock(Duration::from_secs(9));
    view.update(window, |view, cx| view.retry_binary(cx));
    window.run_until_parked();
    assert_page(&view, window, DiffSide::New, 0);
    window.executor().advance_clock(Duration::from_secs(1));
    click(window, "crew-binary-retry-page");
    assert_page(&view, window, DiffSide::New, 3);
    assert_tail_was_retried(&mock, 5);
    click(window, "crew-binary-previous");
    assert_page(&view, window, DiffSide::New, 0);
}

#[gpui_kit::test]
fn reopening_binary_keeps_retry_after_for_the_pinned_side(cx: &mut TestAppContext) {
    let reads = AtomicUsize::new(0);
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("/diff/file?")
            && request.path.contains("&side=new")
            && reads.fetch_add(1, Ordering::Relaxed) == 0
        {
            // Longer than the client's automatic retry budget.
            return (
                503,
                vec![("Retry-After".into(), "11".into())],
                String::new(),
            );
        }
        let (status, body) = binary_response(request);
        (status, vec![], body)
    });
    let new_side_reads = || {
        mock.requests()
            .iter()
            .filter(|req| req.path.contains("/diff/file?") && req.path.contains("&side=new"))
            .count()
    };
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    assert_eq!(new_side_reads(), 1);
    click(window, "crew-binary-close");
    click(window, ("crew-diff-bytes", 0usize));
    assert_eq!(
        new_side_reads(),
        1,
        "reopening sent a read during Retry-After"
    );
    window.update(|window, _| {
        assert_eq!(window.find("crew-binary-retry-page").disabled(), Some(true));
    });

    click(window, ("crew-binary-side", 0usize));
    assert_page(&view, window, DiffSide::Old, 0);
    click(window, ("crew-binary-side", 1usize));
    window.executor().advance_clock(Duration::from_secs(10));
    view.update(window, |view, cx| view.retry_binary(cx));
    window.run_until_parked();
    assert_eq!(new_side_reads(), 1);
    window.executor().advance_clock(Duration::from_secs(1));
    click(window, "crew-binary-retry-page");
    assert_page(&view, window, DiffSide::New, 0);
    assert_eq!(new_side_reads(), 2);
}

fn busy_once_after_open() -> Mock {
    let reads = AtomicUsize::new(0);
    Mock::with_headers(move |request| {
        if request.path.contains("/diff/file?") && reads.fetch_add(1, Ordering::Relaxed) == 1 {
            return (
                503,
                vec![("Retry-After".into(), "10".into())],
                String::new(),
            );
        }
        let (status, body) = binary_response(request);
        (status, vec![], body)
    })
}

#[gpui_kit::test]
fn closing_during_automatic_backoff_keeps_the_pinned_side_held(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mock = busy_once_after_open();
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    assert_page(&view, window, DiffSide::New, 0);
    let first = binary_read_count(&mock);
    let (busy, waiting) = mpsc::channel();
    let (source, client, mode, read) = view.read_with(window, |view, cx| {
        let DiffSource::Fleet(source) = &view.source else {
            panic!("expected Fleet diff")
        };
        let read = view.binary.as_ref().unwrap().closed.clone();
        let read = read.observing_busy(move |seconds| busy.send(seconds).unwrap());
        (
            source.clone(),
            source.client(cx).unwrap(),
            view.fleet.mode,
            read,
        )
    });
    // The visual test executor runs blocking I/O inline. Use the open reader's
    // real token and Blob path on a worker so GPUI can close it during backoff.
    let worker = std::thread::spawn(move || {
        let blob = Blob {
            path: PATH,
            at: "B",
            side: DiffSide::New,
        };
        PreviewRead::default().read(|position| {
            blob.read(&client, source.request(mode), position, &read)
                .map_err(PreviewError::Fleet)
        })
    });
    assert_eq!(waiting.recv_timeout(Duration::from_secs(5)).unwrap(), 10);
    click(window, "crew-binary-close");
    assert!(matches!(
        worker.join().unwrap(),
        Err(PreviewError::Fleet(Error::DiffReadCancelled))
    ));
    click(window, ("crew-diff-bytes", 0usize));
    assert_eq!(
        binary_read_count(&mock) - first,
        1,
        "reopening broke the server's backoff"
    );
    window.update(|window, _| {
        assert_eq!(window.find("crew-binary-retry-page").disabled(), Some(true));
    });
    window.executor().advance_clock(Duration::from_secs(10));
    click(window, "crew-binary-retry-page");
    assert_page(&view, window, DiffSide::New, 0);
    assert_eq!(binary_read_count(&mock) - first, 2);
}

#[gpui_kit::test]
fn only_the_visible_held_side_ticks(cx: &mut TestAppContext) {
    let mock = Mock::with_headers(|request| {
        if request.path.contains("/diff/file?") {
            return (
                503,
                vec![("Retry-After".into(), "11".into())],
                String::new(),
            );
        }
        let (status, body) = binary_response(request);
        (status, vec![], body)
    });
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    click(window, ("crew-binary-side", 0usize));
    click(window, ("crew-binary-side", 1usize));
    window.update(|window, _| {
        assert_eq!(window.find("crew-binary-retry-page").disabled(), Some(true));
    });
    assert_eq!(binary_read_count(&mock), 2);

    let ticks = Rc::new(Cell::new(0));
    let count = ticks.clone();
    let _subscription =
        window.update(|_, cx| cx.observe(&view, move |_, _| count.set(count.get() + 1)));
    window.executor().advance_clock(Duration::from_secs(1));
    window.run_until_parked();
    assert_eq!(ticks.get(), 1, "the hidden held side must not tick");

    click(window, "crew-binary-close");
    ticks.set(0);
    window.executor().advance_clock(Duration::from_secs(1));
    window.run_until_parked();
    assert_eq!(
        ticks.get(),
        0,
        "no countdown remains after the reader closes"
    );
}

fn busy_tail(last_wait: &'static str) -> Mock {
    let tail = AtomicUsize::new(0);
    Mock::with_headers(move |request| {
        if request.path.contains("&page=png-tail") {
            let attempt = tail.fetch_add(1, Ordering::Relaxed);
            if attempt < 3 {
                let wait = if attempt == 2 { last_wait } else { "0" };
                return (
                    503,
                    vec![("Retry-After".into(), wait.into())],
                    String::new(),
                );
            }
        }
        let (status, body) = binary_response(request);
        (status, vec![], body)
    })
}

fn assert_tail_was_retried(mock: &Mock, file_count: usize) {
    let requests = mock.requests();
    let paths: Vec<_> = requests
        .iter()
        .filter(|req| req.path.contains("/diff/file?"))
        .map(|req| req.path.as_str())
        .collect();
    assert_eq!(paths.len(), file_count);
    let tail = &paths[file_count - 2..];
    assert!(tail[0].contains("&page=png-tail"));
    assert_eq!(tail[0], tail[1]);
    assert_eq!(
        requests
            .iter()
            .filter(|req| req.path.contains("/diff?"))
            .count(),
        1
    );
}
