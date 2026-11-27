use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Empty, TestAppContext};
use serde_json::{Value, json};
use subspace_client_fleet::Error;
use subspace_framework::client_test_support::Mock;

use super::{DiffSource, fixture, mount};

#[gpui_kit::test]
fn the_gateway_list_contract_loads_two_pages_or_an_empty_snapshot(cx: &mut TestAppContext) {
    const PAGE: &str = "eyJrIjoibGlzdCIsIm8iOjF9";
    for empty in [false, true] {
        let mock = Mock::new(move |request| {
            let mut response = json!({
                "ship": super::SHIP, "member": super::CREW, "project": null, "mode": "branch",
                "fingerprint": "B", "unstable": false, "branch": "example", "base": null,
                "head": "0123456789abcdef", "total": if empty { 0 } else { 2 },
                "additions": 0, "deletions": 0, "files": [],
            });
            if request.path.ends_with("/diff?mode=branch") {
                if !empty {
                    response["files"] = json!([listed_file("first.txt")]);
                    response["nextPage"] = json!(PAGE);
                }
            } else if !empty
                && request
                    .path
                    .ends_with(&format!("/diff?mode=branch&page={PAGE}"))
            {
                response["files"] = json!([listed_file("second.txt")]);
            } else {
                return (404, String::new());
            }
            (200, response.to_string())
        });
        let (view, window) = mount(cx, &mock);
        view.read_with(window, |view, _| {
            assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
            assert!(view.error.is_none());
            let paths: Vec<_> = view
                .files
                .iter()
                .map(|file| file.file.path.as_str())
                .collect();
            assert_eq!(
                paths,
                if empty {
                    &[][..]
                } else {
                    &["first.txt", "second.txt"]
                }
            );
        });
    }
}

fn listed_file(path: &str) -> Value {
    json!({"path": path, "previousPath": null, "status": "renamed",
           "additions": 0, "deletions": 0, "patch": null})
}

#[gpui_kit::test]
fn a_feed_change_during_a_list_load_is_compared_with_its_result(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.path.contains("/diff?") {
            (200, fixture(false))
        } else {
            (404, String::new())
        }
    });
    let (view, window) = mount(cx, &mock);
    view.update(window, |view, cx| {
        view.observe_crew_fingerprint(Some("A".into()));
        view.load(cx);
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
        assert!(view.refreshing_crew());
        // Deliver the feed event before the newly scheduled HTTP read can apply.
        view.observe_crew_fingerprint(Some("C".into()));
    });
    window.run_until_parked();
    view.read_with(window, |view, _| {
        assert_eq!(view.fleet.fingerprint.as_deref(), Some("B"));
        assert_eq!(view.fleet.banner(), Some("Changed since loaded"));
    });
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.update(|window, _| assert!(window.find("crew-diff-banner-refresh").visible()));
}

#[gpui_kit::test]
fn retries_a_real_503_without_changing_the_pinned_file_request(cx: &mut TestAppContext) {
    let attempts = Arc::new(AtomicUsize::new(0));
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("/diff/file?") {
            if attempts.fetch_add(1, Ordering::Relaxed) == 0 {
                return (503, vec![("Retry-After".into(), "0".into())], String::new());
            }
            let page = json!({"fingerprint": "B", "file": {
                "path": "example.txt", "status": "modified", "patch": "+AFTER-503\n",
            }});
            return (200, vec![], page.to_string());
        }
        if request.path.contains("/diff?") {
            (200, vec![], fixture(false))
        } else {
            (404, vec![], String::new())
        }
    });
    let (view, window) = mount(cx, &mock);
    view.update(window, |view, cx| view.load_more(0, cx));
    window.run_until_parked();
    view.read_with(window, |view, _| {
        assert!(view.fleet.busy_pages.is_empty());
        assert!(view.files[0].next_page.is_none());
        assert!(
            view.files[0]
                .file
                .patch
                .as_deref()
                .unwrap()
                .contains("AFTER-503")
        );
    });
    let paths: Vec<_> = mock
        .requests()
        .into_iter()
        .map(|request| request.path)
        .filter(|path| path.contains("/diff/file?"))
        .collect();
    assert_eq!(paths.len(), 2);
    assert!(paths[0].contains("&at=B&page=page-file"));
    assert_eq!(paths[0], paths[1]);
}

#[gpui_kit::test]
fn releasing_the_view_during_a_continuation_stops_the_next_request(cx: &mut TestAppContext) {
    let paging = Arc::new(AtomicBool::new(false));
    let armed = paging.clone();
    let (started, pending) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    let mock = Mock::new(move |request| {
        if request.path.contains("page=page-list") {
            started.send(()).unwrap();
            wait.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            return (200, middle_page());
        }
        if request.path.contains("/diff?") {
            let mut body: Value =
                serde_json::from_str(&fixture(armed.load(Ordering::Acquire))).unwrap();
            if armed.load(Ordering::Acquire) {
                body["total"] = json!(3);
            }
            return (200, body.to_string());
        }
        (404, String::new())
    });
    let (view, window) = mount(cx, &mock);
    let (source, client, mode, read) = view.read_with(window, |view, cx| {
        let DiffSource::Fleet(source) = &view.source else {
            panic!("expected Fleet diff")
        };
        (
            source.clone(),
            source.client(cx).unwrap(),
            view.fleet.mode,
            view.fleet.list_read.clone(),
        )
    });
    paging.store(true, Ordering::Release);
    // Run the same loader on a real thread: the headless GPUI executor would
    // otherwise block the test's UI thread at the gated HTTP response.
    let worker = std::thread::spawn(move || source.load(&client, mode, &read));
    pending.recv_timeout(Duration::from_secs(5)).unwrap();
    window.update(|window, cx| {
        window.replace_root(cx, |_, _| Empty);
    });
    drop(view);
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.run_until_parked();
    release.send(()).unwrap();
    assert_eq!(worker.join().unwrap().err(), Some(Error::DiffReadCancelled));
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.contains("page=never"))
    );
}

fn middle_page() -> String {
    let mut body: Value = serde_json::from_str(&fixture(true)).unwrap();
    body["total"] = json!(3);
    body["nextPage"] = json!("never");
    body["files"][0]["path"] = json!("next.txt");
    body.to_string()
}
