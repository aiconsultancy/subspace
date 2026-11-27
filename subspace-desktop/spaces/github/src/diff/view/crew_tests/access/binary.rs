use std::sync::Mutex;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_client_fleet::diff::DiffMode;
use subspace_framework::client_test_support::Mock;

use super::{ReadGate, ThreadedReads, assert_denial_display, mount};
use crate::diff::view::crew_tests::{CREW, SHIP};

const IMAGE: &str = "icon.png";
const REPLACEMENT: &str = "replacement.dat";

#[gpui_kit::test]
fn a_current_byte_403_clears_the_snapshot_and_reader(cx: &mut TestAppContext) {
    check_byte_403(cx, false);
}

#[gpui_kit::test]
fn a_superseded_byte_403_keeps_the_new_snapshot_and_reader(cx: &mut TestAppContext) {
    check_byte_403(cx, true);
}

fn check_byte_403(cx: &mut TestAppContext, superseded: bool) {
    let (gate, pending, release) = ReadGate::new();
    let gate = Mutex::new(gate);
    let mock = Mock::new(move |request| {
        if request.path.contains("/diff/file?") {
            if request.path.contains("mode=working") {
                assert!(request.path.contains("&at=C&side=new"));
                return (200, first_page("C", REPLACEMENT, false));
            }
            assert!(request.path.contains("&at=B&side=new"));
            gate.lock().unwrap().pause();
            return (403, String::new());
        }
        list_reply(&request.path)
    });
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    window.update(|window, cx| window.click(("crew-diff-bytes", 0usize), cx));
    window.run_until_parked();
    pending.recv_timeout(Duration::from_secs(5)).unwrap();
    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
        assert!(view.binary.is_some());
    });
    if superseded {
        view.update(window, |view, cx| {
            view.set_mode(DiffMode::Working, None, cx)
        });
        ThreadedReads::finish_last(window);
        redraw(window);
        window.update(|window, cx| window.click(("crew-diff-bytes", 0usize), cx));
        window.run_until_parked();
        ThreadedReads::finish_last(window);
    }
    release.send(()).unwrap();
    ThreadedReads::finish_last(window);
    if superseded {
        view.read_with(window, |view, _| {
            assert_eq!(view.fleet.fingerprint.as_deref(), Some("C"));
            assert_eq!(view.files[0].file.path, REPLACEMENT);
            assert!(view.binary.is_some());
            assert!(view.error.is_none());
        });
    } else {
        view.read_with(window, |view, _| assert_denied(view));
        assert_denial_display(window, 403);
    }
}

#[gpui_kit::test]
fn a_current_preview_403_clears_the_snapshot_and_reader(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.path.contains("/diff/file?") {
            assert!(request.path.contains("&at=B") && request.path.ends_with("&side=new"));
            if request.path.contains("&page=preview-tail") {
                return (403, String::new());
            }
            return (200, first_page("B", IMAGE, true));
        }
        list_reply(&request.path)
    });
    let (view, window) = mount(cx, &mock);
    window.update(|window, cx| window.click(("crew-diff-bytes", 0usize), cx));
    window.run_until_parked();
    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
        assert!(view.binary.is_some());
    });
    redraw(window);
    window.update(|window, cx| window.click("crew-binary-preview", cx));
    window.run_until_parked();
    view.read_with(window, |view, _| assert_denied(view));
    assert_denial_display(window, 403);
}

fn assert_denied(view: &crate::diff::view::DiffView) {
    assert!(!view.loaded, "a denied read must clear the snapshot");
    assert!(view.files.is_empty());
    assert!(view.fleet.fingerprint.is_none());
    assert!(view.binary.is_none());
}

fn redraw(window: &mut VisualTestContext) {
    window.update(|window, cx| {
        gpui_kit::base::test_support::begin_frame(window);
        window.draw(cx).clear(cx);
    });
}

fn list_reply(path: &str) -> (u16, String) {
    let (mode, fingerprint, path) = if path.ends_with("/diff?mode=working") {
        ("working", "C", REPLACEMENT)
    } else if path.ends_with("/diff?mode=branch") {
        ("branch", "B", IMAGE)
    } else {
        return (404, String::new());
    };
    (
        200,
        json!({
            "ship": SHIP, "member": CREW, "mode": mode, "fingerprint": fingerprint,
            "total": 1, "additions": 0, "deletions": 0, "files": [binary_file(path)],
        })
        .to_string(),
    )
}

fn binary_file(path: &str) -> Value {
    json!({"path": path, "status": "modified", "binary": true})
}

fn first_page(fingerprint: &str, path: &str, more: bool) -> String {
    // PNG signature prefix. The denied continuation never supplies the rest.
    json!({"fingerprint": fingerprint, "file": binary_file(path), "content": {
        "side": "new", "encoding": "base64", "offset": 0, "data": "iVBO",
        "size": if more { 70 } else { 3 }, "nextPage": more.then_some("preview-tail"),
    }})
    .to_string()
}
