use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_framework::client_test_support::Mock;

use super::{DiffView, ReadGate, fixture, mount, threaded_reads::ThreadedReads};

#[gpui_kit::test]
fn an_unchanged_refresh_cannot_clear_a_page_conflict_that_arrived_later(cx: &mut TestAppContext) {
    check_late_page_notice(cx, 409, String::new(), "Changed since loaded");
}

#[gpui_kit::test]
fn an_unchanged_refresh_cannot_clear_a_page_warning_that_arrived_later(cx: &mut TestAppContext) {
    let response = json!({"fingerprint": "B", "unstable": true, "file": {
        "path": "example.txt", "status": "modified", "patch": "+LATE-WARNING\n",
    }});
    check_late_page_notice(cx, 200, response.to_string(), "Changed while loading");
}

fn check_late_page_notice(cx: &mut TestAppContext, status: u16, body: String, notice: &str) {
    let (page, page_pending, release_page) = ReadGate::new();
    let (refresh, refresh_pending, release_refresh) = ReadGate::new();
    let (page, refresh) = (Mutex::new(page), Mutex::new(refresh));
    let lists = AtomicUsize::new(0);
    let mock = Mock::new(move |request| {
        if request.path.contains("/diff/file?") {
            page.lock().unwrap().pause();
            return (status, body.clone());
        }
        if request.path.contains("/diff?") {
            if lists.fetch_add(1, Ordering::Relaxed) == 1 {
                refresh.lock().unwrap().pause();
            }
            return (200, fixture(false));
        }
        (404, String::new())
    });
    // Both senders drop before Mock, even if a test assertion unwinds.
    let (release_page, release_refresh) = (release_page, release_refresh);
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    view.update(window, |view, cx| view.load_more(0, cx));
    page_pending.recv_timeout(Duration::from_secs(5)).unwrap();
    view.update(window, |view, cx| view.load(cx));
    refresh_pending
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    release_page.send(()).unwrap();
    ThreadedReads::finish(window, 0);
    release_refresh.send(()).unwrap();
    ThreadedReads::finish_last(window);
    check_notice(&view, window, notice, status == 409);
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.path.contains("/diff/file?"))
            .count(),
        1
    );
    view.update(window, |view, cx| view.load(cx));
    ThreadedReads::finish_last(window);
    view.read_with(window, |view, _| {
        assert!(view.fleet.banner().is_none());
        assert!(!view.crew_pages_suspended());
    });
}

fn check_notice(
    view: &Entity<DiffView>,
    window: &mut VisualTestContext,
    notice: &str,
    stale: bool,
) {
    view.update(window, |view, cx| {
        assert_eq!(view.fleet.banner(), Some(notice));
        assert_eq!(view.crew_pages_suspended(), stale);
        if stale {
            view.load_more(0, cx);
        } else {
            assert!(
                view.files[0]
                    .file
                    .patch
                    .as_deref()
                    .unwrap()
                    .contains("LATE-WARNING")
            );
        }
    });
}
