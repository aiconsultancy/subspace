use std::sync::Mutex;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use subspace_client_fleet::diff::DiffMode;
use subspace_framework::client_test_support::Mock;

use super::{ReadGate, fixture, mount, refresh::snapshot, threaded_reads::ThreadedReads};

#[gpui_kit::test]
fn a_current_patch_401_clears_the_snapshot(cx: &mut TestAppContext) {
    check_patch_reply(cx, 401, false);
}

#[gpui_kit::test]
fn a_current_patch_403_clears_the_snapshot(cx: &mut TestAppContext) {
    check_patch_reply(cx, 403, false);
}

#[gpui_kit::test]
fn a_superseded_patch_403_keeps_the_new_snapshot(cx: &mut TestAppContext) {
    check_patch_reply(cx, 403, true);
}

#[gpui_kit::test]
fn a_transient_patch_failure_keeps_the_snapshot(cx: &mut TestAppContext) {
    check_patch_reply(cx, 500, false);
}

fn check_patch_reply(cx: &mut TestAppContext, status: u16, superseded: bool) {
    let (gate, pending, release) = ReadGate::new();
    let gate = Mutex::new(gate);
    let mock = Mock::new(move |request| {
        if request.path.contains("/diff/file?") {
            gate.lock().unwrap().pause();
            return (status, String::new());
        }
        if request.path.contains("mode=working") {
            return (200, snapshot("working", "C", &["working.txt"]));
        }
        (200, fixture(false))
    });
    let release_page = release;
    let (view, window) = mount(cx, &mock);
    ThreadedReads::install(window);
    view.update(window, |view, cx| view.load_more(0, cx));
    pending.recv_timeout(Duration::from_secs(5)).unwrap();
    if superseded {
        view.update(window, |view, cx| {
            view.set_mode(DiffMode::Working, None, cx)
        });
        ThreadedReads::finish_last(window);
    }
    release_page.send(()).unwrap();
    ThreadedReads::finish_last(window);
    view.read_with(window, |view, _| match (status, superseded) {
        (_, true) => {
            assert_eq!(view.fleet.fingerprint.as_deref(), Some("C"));
            assert_eq!(view.files[0].file.path, "working.txt");
            assert!(view.error.is_none());
        }
        (500, _) => {
            assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
            assert_eq!(view.files[0].next_page.as_deref(), Some("page-file"));
        }
        _ => {
            assert!(!view.loaded);
            assert!(view.files.is_empty());
            assert!(view.fleet.fingerprint.is_none());
            assert!(
                view.error
                    .as_deref()
                    .unwrap()
                    .contains(&format!("({status})"))
            );
        }
    });
    if !superseded && status != 500 {
        assert_denial_display(window, status);
    }
}

fn assert_denial_display(window: &mut VisualTestContext, status: u16) {
    window.update(|window, cx| {
        gpui_kit::base::test_support::begin_frame(window);
        window.draw(cx).clear(cx);
        assert!(window.find("crew-diff-error-refresh").visible());
        let texts = gpui_kit::base::test_support::texts(window);
        let status = format!("({status})");
        assert!(
            texts
                .iter()
                .any(|text| text.visible() && text.text().contains(&status))
        );
    });
}
