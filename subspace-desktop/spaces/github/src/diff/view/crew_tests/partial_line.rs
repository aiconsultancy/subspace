use std::sync::Mutex;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

use super::super::super::model::Layout;
use super::{DiffView, ReadGate, fixture, mount, threaded_reads::ThreadedReads};

#[gpui_kit::test]
fn the_open_partial_line_reader_updates_when_the_rest_of_the_line_arrives(cx: &mut TestAppContext) {
    let (gate, pending, release) = ReadGate::new();
    let gate = Mutex::new(gate);
    let mock = Mock::new(move |request| {
        if request.path.contains("/diff/file?") {
            gate.lock().unwrap().pause();
            return (
                200,
                json!({ "fingerprint": "B", "file": {
                    "path": "example.txt", "status": "modified", "patch": "suffix\n" }
                })
                .to_string(),
            );
        }
        let mut body: Value = serde_json::from_str(&fixture(false)).unwrap();
        body["files"][0]["patch"] = json!("@@ -0,0 +1 @@\n+prefix-");
        (200, body.to_string())
    });
    // Drop the sender before Mock on unwind, so the worker cannot stay gated.
    let release_page = release;
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    view.update(window, |view, cx| {
        view.set_layout(Layout::Unified, cx);
        view.load_more(0, cx);
    });
    pending.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(partial_line_visible(window));
    window.update(|window, cx| {
        window.click(("diff-line-left", 0usize), cx);
    });
    check_line_reader(&view, window, "Partial line", "prefix-");
    release_page.send(()).unwrap();
    ThreadedReads::finish_last(window);
    check_line_reader(&view, window, "Full line", "prefix-suffix");
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.update(|window, cx| window.click("diff-full-line-close", cx));
    assert!(!partial_line_visible(window));
}

fn check_line_reader(
    view: &Entity<DiffView>,
    window: &mut VisualTestContext,
    title: &str,
    text: &str,
) {
    view.read_with(window, |view, _| {
        let line = view.full_line.as_ref().expect("the reader is still open");
        assert_eq!(line.title(), title);
        assert_eq!(line.parts().0, text);
    });
}

fn partial_line_visible(window: &mut VisualTestContext) -> bool {
    window.update(|window, cx| {
        // This test mounts the Space directly, without the Kit root that
        // normally starts a fresh text observation frame.
        gpui_kit::base::test_support::begin_frame(window);
        window.draw(cx).clear(cx);
        assert!(window.find(("diff-line-left", 0usize)).visible());
        gpui_kit::base::test_support::texts(window)
            .iter()
            .any(|text| text.visible() && text.text() == "Partial line")
    })
}
