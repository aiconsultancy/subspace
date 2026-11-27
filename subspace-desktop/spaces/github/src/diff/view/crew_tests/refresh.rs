use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, Entity, Global, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_client_fleet::diff::DiffMode;
use subspace_framework::client_test_support::Mock;
use tokio::sync::oneshot;

use super::{DiffView, ReadGate, fixture, mount};

/// The test dispatcher runs blocking work on the UI thread. Opt in to a real
/// worker so the test can draw and interact while an HTTP response is gated.
#[derive(Default)]
pub(in crate::diff::view) struct ThreadedReads(Vec<JoinHandle<DeliverRead>>);

type DeliverRead = Box<dyn FnOnce() + Send>;

impl Global for ThreadedReads {}

impl ThreadedReads {
    pub(in crate::diff::view) fn start_if_installed<T: Send + 'static>(
        cx: &mut App,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Option<oneshot::Receiver<T>> {
        cx.try_global::<Self>()?;
        let (tx, rx) = oneshot::channel();
        cx.global_mut::<Self>()
            .0
            .push(thread::spawn(move || -> DeliverRead {
                let result = work();
                Box::new(move || {
                    _ = tx.send(result);
                })
            }));
        Some(rx)
    }

    fn finish_last(window: &mut VisualTestContext) {
        // Only the UI test thread can wake its deterministic scheduler.
        let deliver =
            window.update(|_, cx| cx.global_mut::<Self>().0.pop().unwrap().join().unwrap());
        deliver();
        window.run_until_parked();
    }
}

fn snapshot(mode: &str, at: &str, files: &[&str]) -> String {
    let mut body: Value = serde_json::from_str(&fixture(false)).unwrap();
    let mut file = body["files"][0].clone();
    let patch = format!("@@ -0,0 +1,80 @@\n{}", "+example.com\n".repeat(80));
    file["patch"] = json!(patch);
    body["mode"] = json!(mode);
    body["fingerprint"] = json!(at);
    body["total"] = json!(files.len());
    body["files"] = Value::Array(
        files
            .iter()
            .map(|path| {
                let mut file = file.clone();
                file["path"] = json!(path);
                file
            })
            .collect(),
    );
    body.to_string()
}

fn top_row(view: &DiffView, cx: &App) -> usize {
    let tokens = subspace_framework::dashboard::Tokens::current(cx);
    view.top_row(super::super::code_row_height(&tokens))
}

fn check_old_view(view: &Entity<DiffView>, window: &mut VisualTestContext, old_top: usize) {
    view.read_with(window, |view, cx| {
        assert!(
            view.loaded,
            "same-scope refresh cleared the displayed snapshot"
        );
        assert_eq!(view.files[view.current].file.path, "target.txt");
        assert_eq!(top_row(view, cx), old_top);
    });
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(window.find(("tree-file", 0usize)).visible());
    });
}

#[gpui_kit::test]
fn refresh_keeps_selection_and_position_during_http_and_after_failure(cx: &mut TestAppContext) {
    let (gate, pending, release) = ReadGate::new();
    let gate = Mutex::new(gate);
    let reads = AtomicUsize::new(0);
    let mock = Mock::new(move |request| match request.path.contains("/diff?") {
        false => (404, String::new()),
        true => match reads.fetch_add(1, Ordering::Relaxed) {
            0 => (200, snapshot("branch", "B", &["target.txt"])),
            1 => {
                gate.lock().unwrap().pause();
                (500, String::new())
            }
            _ => (200, snapshot("branch", "C", &["new.txt", "target.txt"])),
        },
    });
    let (view, window) = mount(cx, &mock);
    window.update(|_, cx| cx.set_global(ThreadedReads::default()));
    view.update(window, |view, cx| view.scroll_by(5., cx));
    let old_top = view.read_with(window, top_row);
    assert!(old_top > 0);
    window.update(|window, cx| window.click("crew-diff-refresh", cx));
    pending.recv_timeout(Duration::from_secs(5)).unwrap();
    check_old_view(&view, window, old_top);
    view.update(window, |view, cx| view.load_more(0, cx));
    window.run_until_parked();
    release.send(()).unwrap();
    ThreadedReads::finish_last(window);
    check_old_view(&view, window, old_top);
    assert!(
        !mock
            .requests()
            .iter()
            .any(|req| req.path.contains("/diff/file?"))
    );
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(window.find("crew-diff-refresh-retry").visible());
        window.click("crew-diff-refresh-retry", cx);
    });
    ThreadedReads::finish_last(window);
    window.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(window, |view, cx| {
        assert!(view.error.is_none());
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("C"));
        assert_eq!(view.files[view.current].file.path, "target.txt");
        assert_eq!(top_row(view, cx), view.starts[1] + old_top);
    });
}

#[gpui_kit::test]
fn refresh_drops_the_gated_response_after_a_mode_switch(cx: &mut TestAppContext) {
    let (gate, pending, release) = ReadGate::new();
    let gate = Mutex::new(gate);
    let reads = AtomicUsize::new(0);
    let mock = Mock::new(move |request| {
        if request.path.contains("mode=working") {
            return (200, snapshot("working", "C", &["working.txt"]));
        }
        if !request.path.contains("/diff?") {
            return (404, String::new());
        }
        if reads.fetch_add(1, Ordering::Relaxed) == 0 {
            return (200, snapshot("branch", "B", &["initial.txt"]));
        }
        gate.lock().unwrap().pause();
        (200, snapshot("branch", "late", &["superseded.txt"]))
    });
    let (view, window) = mount(cx, &mock);
    window.update(|_, cx| cx.set_global(ThreadedReads::default()));
    window.update(|window, cx| window.click("crew-diff-refresh", cx));
    pending.recv_timeout(Duration::from_secs(5)).unwrap();
    view.update(window, |view, cx| {
        view.set_mode(DiffMode::Working, None, cx);
        assert!(!view.loaded);
    });
    window.run_until_parked();
    ThreadedReads::finish_last(window);
    release.send(()).unwrap();
    ThreadedReads::finish_last(window);
    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("C"));
        assert_eq!(view.files[0].file.path, "working.txt");
    });
}

#[gpui_kit::test]
fn refresh_clears_the_snapshot_on_confirmed_access_or_checkout_loss(cx: &mut TestAppContext) {
    for status in [401, 403, 404] {
        let reads = AtomicUsize::new(0);
        let mock = Mock::new(move |request| {
            if request.path.contains("/diff?") && reads.fetch_add(1, Ordering::Relaxed) == 0 {
                return (200, snapshot("branch", "B", &["initial.txt"]));
            }
            (status, String::new())
        });
        let (view, window) = mount(cx, &mock);
        view.read_with(window, |view, _| assert_eq!(view.files.len(), 1));
        window.update(|window, cx| window.click("crew-diff-refresh", cx));
        window.run_until_parked();
        view.read_with(window, |view, _| {
            assert!(view.files.is_empty());
            assert!(view.fleet.fingerprint.is_none());
            assert_eq!(view.fleet.unavailable, status == 404);
            assert_eq!(view.error.is_some(), status != 404);
        });
    }
}
