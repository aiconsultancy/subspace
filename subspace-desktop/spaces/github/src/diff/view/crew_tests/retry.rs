use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::json;
use subspace_client_fleet::{Error, diff::DiffFilePage};
use subspace_framework::client_test_support::Mock;

use super::{DiffSource, fixture, mount, refresh::ThreadedReads};

/// Blocking HTTP runs on a real thread so the visual test can deliver a feed
/// update while the simulated GPUI executor is otherwise idle.
pub(in crate::diff::view) struct ReadGate {
    reached: Sender<()>,
    resume: Receiver<()>,
}

impl ReadGate {
    pub(in crate::diff::view) fn new() -> (Self, Receiver<()>, Sender<()>) {
        let (reached, waiting) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        (Self { reached, resume }, waiting, release)
    }

    pub(in crate::diff::view) fn pause(&self) {
        self.reached.send(()).unwrap();
        self.resume.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}

#[gpui_kit::test]
fn a_live_change_cancels_a_pinned_file_503_without_cancelling_list_reads(cx: &mut TestAppContext) {
    let mock = Mock::with_headers(|request| {
        if request.path.contains("/diff/file?") {
            (
                503,
                vec![("Retry-After".into(), "10".into())],
                String::new(),
            )
        } else if request.path.contains("/diff?") {
            (200, vec![], fixture(false))
        } else {
            (404, vec![], String::new())
        }
    });
    let (view, window) = mount(cx, &mock);
    let (source, client, mode, read, list) = view.read_with(window, |view, cx| {
        let DiffSource::Fleet(source) = &view.source else {
            panic!("expected Fleet diff")
        };
        (
            source.clone(),
            source.client(cx).unwrap(),
            view.fleet.mode,
            view.fleet.page_read.clone(),
            view.fleet.list_read.clone(),
        )
    });
    let (gate, waiting, release) = ReadGate::new();
    let worker = std::thread::spawn(move || {
        read.retry(|| {
            let result = client.diff_file(
                source.request(mode),
                DiffFilePage {
                    path: "example.txt",
                    at: "B",
                    page: Some("page-file"),
                    side: None,
                },
            );
            assert_eq!(result.as_ref().err(), Some(&Error::Busy(Some(10))));
            gate.pause();
            result
        })
    });
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    // Release the observed 503 into the retry loop; the new fingerprint
    // cancels it at or during its backoff, before another request is sent.
    release.send(()).unwrap();
    view.update(window, |view, _| {
        assert!(view.observe_crew_fingerprint(Some("C".into())));
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
        assert_eq!(view.files[0].next_page.as_deref(), Some("page-file"));
    });
    assert_eq!(list.check(), Ok(()));
    assert_eq!(worker.join().unwrap().err(), Some(Error::DiffReadCancelled));
    assert_eq!(file_requests(&mock).len(), 1);
}

#[gpui_kit::test]
fn retry_page_repeats_only_the_failed_continuation_and_keeps_loaded_patches(
    cx: &mut TestAppContext,
) {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("&page=page-file") {
            return (
                200,
                vec![],
                patch_page("+FIRST-CONTINUATION\n", Some("failed")),
            );
        }
        if request.path.contains("&page=failed") {
            if count.fetch_add(1, Ordering::Relaxed) < 3 {
                return (503, vec![("Retry-After".into(), "0".into())], String::new());
            }
            return (200, vec![], patch_page("+AFTER-RETRY\n", None));
        }
        if request.path.contains("/diff?") {
            return (200, vec![], fixture(false));
        }
        (404, vec![], String::new())
    });
    let (view, window) = mount(cx, &mock);
    for _ in 0..2 {
        view.update(window, |view, cx| view.load_more(0, cx));
        window.run_until_parked();
    }
    let before = view.read_with(window, |view, _| {
        assert!(view.fleet.busy_pages.contains_key(&0));
        let patch = view.files[0].file.patch.clone().unwrap();
        assert!(patch.contains("FIRST-CONTINUATION"));
        patch
    });
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(window.find("crew-diff-banner-refresh").visible());
        assert!(window.find(("crew-diff-retry-page", 0usize)).visible());
        window.click(("crew-diff-retry-page", 0usize), cx);
    });
    window.run_until_parked();
    view.read_with(window, |view, _| {
        assert!(view.fleet.busy_pages.is_empty());
        assert_eq!(
            view.files[0].file.patch.as_deref().unwrap(),
            before + "+AFTER-RETRY\n"
        );
    });
    let paths = file_requests(&mock);
    assert_eq!(paths.len(), 5);
    assert!(paths[1].contains("&at=B&page=failed"));
    assert!(paths[1..].iter().all(|path| path == &paths[1]));
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|req| req.path.contains("/diff?"))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn text_retry_after_ten_disables_retry_and_blocks_reads_until_due(cx: &mut TestAppContext) {
    let attempts = AtomicUsize::new(0);
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("/diff/file?") {
            let attempt = attempts.fetch_add(1, Ordering::Relaxed);
            if attempt < 3 {
                let wait = if attempt == 2 { "10" } else { "0" };
                return (
                    503,
                    vec![("Retry-After".into(), wait.into())],
                    String::new(),
                );
            }
            return (200, vec![], patch_page("+AFTER-RETRY\n", None));
        }
        (200, vec![], fixture(false))
    });
    let (view, window) = mount(cx, &mock);
    view.update(window, |view, cx| view.load_more(0, cx));
    window.run_until_parked();
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(
            window.find(("crew-diff-retry-page", 0usize)).disabled(),
            Some(true)
        );
    });
    window.executor().advance_clock(Duration::from_secs(9));
    view.update(window, |view, cx| view.load_more(0, cx));
    window.run_until_parked();
    assert_eq!(file_requests(&mock).len(), 3);
    window.executor().advance_clock(Duration::from_secs(1));
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.click(("crew-diff-retry-page", 0usize), cx);
    });
    window.run_until_parked();
    assert_eq!(file_requests(&mock).len(), 4);
}

#[gpui_kit::test]
fn unchanged_refresh_keeps_the_pending_patch_page(cx: &mut TestAppContext) {
    let (gate, waiting, release) = ReadGate::new();
    let gate = Mutex::new(gate);
    let mock = Mock::new(move |request| {
        if request.path.contains("/diff/file?") {
            assert!(request.path.contains("&at=B&page=page-file"));
            gate.lock().unwrap().pause();
            return (200, patch_page("+AFTER-REFRESH\n", None));
        }
        (200, fixture(false))
    });
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    window.update(|window, cx| window.click(("crew-diff-more", 0usize), cx));
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    window.update(|window, cx| window.click("crew-diff-refresh", cx));
    ThreadedReads::finish_last(window);
    release.send(()).unwrap();
    ThreadedReads::finish_last(window);
    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
        assert!(view.fleet.loading.is_empty());
        assert!(
            view.files[0].next_page.is_none(),
            "refresh lost the pending patch page"
        );
        assert!(
            view.files[0]
                .file
                .patch
                .as_deref()
                .unwrap()
                .contains("AFTER-REFRESH")
        );
    });
    assert_eq!(file_requests(&mock).len(), 1);
}

#[gpui_kit::test]
fn unchanged_refresh_keeps_the_patch_retry_countdown(cx: &mut TestAppContext) {
    let mock = Mock::with_headers(|request| {
        if request.path.contains("/diff/file?") {
            return (
                429,
                vec![("Retry-After".into(), "30".into())],
                String::new(),
            );
        }
        (200, vec![], fixture(false))
    });
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    window.update(|window, cx| window.click(("crew-diff-more", 0usize), cx));
    ThreadedReads::finish_last(window);
    let remaining = |view: &super::DiffView, cx: &gpui_kit::App| {
        view.fleet.busy_pages.get(&0).map(|busy| busy.remaining(cx))
    };
    assert_eq!(view.read_with(window, remaining), Some(30));
    window.executor().advance_clock(Duration::from_secs(5));
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.click("crew-diff-refresh", cx);
    });
    ThreadedReads::finish_last(window);
    assert_eq!(
        view.read_with(window, remaining),
        Some(25),
        "refresh lost the patch retry countdown"
    );
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(
            window.find(("crew-diff-retry-page", 0usize)).disabled(),
            Some(true)
        );
    });
    assert_eq!(file_requests(&mock).len(), 1);
}

fn patch_page(patch: &str, next: Option<&str>) -> String {
    json!({"fingerprint": "B", "unstable": next.is_some(), "file": {
        "path": "example.txt", "status": "modified", "patch": patch,
        "patchNextPage": next,
    }})
    .to_string()
}

fn file_requests(mock: &Mock) -> Vec<String> {
    mock.requests()
        .into_iter()
        .map(|request| request.path)
        .filter(|path| path.contains("/diff/file?"))
        .collect()
}
