use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

use super::{fixture, mount, threaded_reads::ThreadedReads};

mod backoff;

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
        assert_ne!(
            self.resume.recv_timeout(Duration::from_secs(5)),
            Err(RecvTimeoutError::Timeout),
            "test did not release the HTTP gate",
        );
    }
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

#[gpui_kit::test]
fn changed_refresh_discards_the_pending_patch_page(cx: &mut TestAppContext) {
    let (gate, waiting, release) = ReadGate::new();
    let gate = Mutex::new(gate);
    let lists = AtomicUsize::new(0);
    let mock = Mock::new(move |request| {
        if request.path.contains("&at=C&page=new-file-page") {
            return (200, replacement_page());
        }
        if request.path.contains("/diff/file?") {
            assert!(request.path.contains("&at=B&page=page-file"));
            gate.lock().unwrap().pause();
            return (200, patch_page("+OLD-CONTINUATION\n", None));
        }
        (200, refresh_snapshot(&lists))
    });
    let release_page = release;
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    window.update(|window, cx| window.click(("crew-diff-more", 0usize), cx));
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    view.read_with(window, |view, _| assert!(view.fleet.loading.contains(&0)));
    window.update(|window, cx| window.click("crew-diff-refresh", cx));
    ThreadedReads::finish_last(window);
    release_page.send(()).unwrap();
    ThreadedReads::finish_last(window);

    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("C"));
        assert_eq!(view.files[0].file.path, "replacement.txt");
        assert_eq!(view.files[0].next_page.as_deref(), Some("new-file-page"));
        assert_eq!(
            view.files[0].file.patch.as_deref(),
            Some("@@ -0,0 +1 @@\n+NEW\n")
        );
        assert!(
            view.fleet.loading.is_empty(),
            "old loading flag survived replacement"
        );
    });
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        let more = ("crew-diff-more", 0usize);
        assert!(window.find(more).visible());
        window.click(more, cx);
    });
    view.read_with(window, |view, _| assert!(view.fleet.loading.contains(&0)));
    ThreadedReads::finish_last(window);
    let requests = file_requests(&mock);
    assert_eq!(requests.len(), 2);
    assert!(requests[1].contains("path=replacement.txt&at=C&page=new-file-page"));
}

#[gpui_kit::test]
fn changed_refresh_discards_the_patch_retry_countdown(cx: &mut TestAppContext) {
    let lists = AtomicUsize::new(0);
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("/diff/file?") {
            return (
                429,
                vec![("Retry-After".into(), "30".into())],
                String::new(),
            );
        }
        (200, vec![], refresh_snapshot(&lists))
    });
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    window.update(|window, cx| window.click(("crew-diff-more", 0usize), cx));
    ThreadedReads::finish_last(window);
    view.read_with(window, |view, cx| {
        assert_eq!(view.fleet.busy_pages[&0].remaining(cx), 30)
    });
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        window.click("crew-diff-refresh", cx);
    });
    ThreadedReads::finish_last(window);

    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("C"));
        assert!(
            view.fleet.busy_pages.is_empty(),
            "old Retry-After countdown survived replacement"
        );
    });
    assert_eq!(file_requests(&mock).len(), 1);
}

fn refresh_snapshot(lists: &AtomicUsize) -> String {
    if lists.fetch_add(1, Ordering::Relaxed) == 0 {
        return fixture(false);
    }
    let mut body: Value = serde_json::from_str(&fixture(false)).unwrap();
    body["fingerprint"] = json!("C");
    body["files"][0]["path"] = json!("replacement.txt");
    body["files"][0]["patch"] = json!("@@ -0,0 +1 @@\n+NEW\n");
    body["files"][0]["patchNextPage"] = json!("new-file-page");
    body.to_string()
}

fn replacement_page() -> String {
    json!({"fingerprint": "C", "file": {
        "path": "replacement.txt", "status": "modified", "patch": "+C-CONTINUATION\n",
    }})
    .to_string()
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
