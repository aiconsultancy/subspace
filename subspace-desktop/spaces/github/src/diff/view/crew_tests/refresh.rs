use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_client_fleet::diff::DiffMode;
use subspace_framework::client_test_support::Mock;

use super::super::super::model::{DiffRow, Layout};
use super::super::Text;
use super::{DiffView, ReadGate, fixture, mount, threaded_reads::ThreadedReads};

pub(super) fn snapshot(mode: &str, at: &str, files: &[&str]) -> String {
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
    ThreadedReads::install(window);
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
    ThreadedReads::install(window);
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

#[gpui_kit::test]
fn an_unchanged_refresh_cannot_clear_a_page_conflict_that_arrived_later(cx: &mut TestAppContext) {
    let (page, page_pending, release_page) = ReadGate::new();
    let (refresh, refresh_pending, release_refresh) = ReadGate::new();
    let (page, refresh) = (Mutex::new(page), Mutex::new(refresh));
    let lists = AtomicUsize::new(0);
    let mock = Mock::new(move |request| {
        if request.path.contains("/diff/file?") {
            page.lock().unwrap().pause();
            return (409, String::new());
        }
        if request.path.contains("/diff?") {
            if lists.fetch_add(1, Ordering::Relaxed) == 1 {
                refresh.lock().unwrap().pause();
            }
            return (200, fixture(false));
        }
        (404, String::new())
    });
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
    view.update(window, |view, cx| {
        assert_eq!(view.fleet.banner(), Some("Changed since loaded"));
        assert!(view.crew_pages_suspended());
        view.load_more(0, cx);
    });
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

#[gpui_kit::test]
fn unchanged_refresh_and_retry_keep_the_open_snapshot(cx: &mut TestAppContext) {
    let reads = AtomicUsize::new(0);
    let mock = Mock::new(move |request| unchanged_response(&request.path, &reads));
    let (view, window) = mount(cx, &mock);
    let first_page_lines = view.update(window, |view, cx| {
        view.set_layout(Layout::Unified, cx);
        view.toggle_collapsed(0, cx);
        // Fleet has no context-fetch action. Seed known text to exercise the
        // shared hunk expansion state without adding a Fleet UI feature.
        view.texts
            .insert(1, Text::Ready(Arc::new(vec!["context".to_owned(); 201])));
        view.expand(1, 1, cx);
        view.load_more(1, cx);
        view.files[1].patch.lines.len()
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    view.update(window, |view, cx| {
        view.scroll_by(first_page_lines as f32 + 8., cx)
    });
    window.update(|window, cx| window.draw(cx).clear(cx));
    let (row, line) = view.read_with(window, |view, cx| {
        let row = top_row(view, cx);
        let DiffRow::Line { file: 1, line } = view.rows[row + 1] else {
            panic!("expected a patch row just below the top of the list");
        };
        assert!(line > first_page_lines, "scrolled into the continuation");
        assert!(view.expanded.contains(&(1, 1)));
        (row, line)
    });
    window.update(|window, cx| window.click(("diff-line-left", line), cx));
    window.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(window, |view, _| {
        assert_eq!(view.full_line.as_ref().unwrap().parts().0, "continuation");
    });
    window.update(|window, cx| window.click("crew-diff-refresh", cx));
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.update(|window, cx| window.click("crew-diff-refresh-retry", cx));
    window.run_until_parked();
    view.read_with(window, |view, cx| {
        assert!(view.error.is_none());
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
        assert!(
            view.collapsed.contains(&0),
            "unchanged refresh unfolded a file"
        );
        assert!(view.expanded.contains(&(1, 1)));
        assert_eq!(view.files[1].patch.lines.len(), first_page_lines + 120);
        assert!(view.files[1].next_page.is_none());
        assert_eq!(top_row(view, cx), row);
        assert_eq!(view.full_line.as_ref().unwrap().parts().0, "continuation");
    });
}

fn unchanged_response(path: &str, reads: &AtomicUsize) -> (u16, String) {
    if path.contains("/diff/file?") {
        assert!(path.contains("&at=B&page=page-file"));
        let patch = format!("@@ -1,0 +81,120 @@\n{}", "+continuation\n".repeat(120));
        let page = json!({"fingerprint": "B", "file": {
            "path": "target.txt", "status": "modified", "patch": patch,
        }});
        return (200, page.to_string());
    }
    if !path.contains("/diff?") {
        return (404, String::new());
    }
    if reads.fetch_add(1, Ordering::Relaxed) == 1 {
        return (500, String::new());
    }
    (
        200,
        snapshot("branch", "B", &["collapsed.txt", "target.txt"]),
    )
}
