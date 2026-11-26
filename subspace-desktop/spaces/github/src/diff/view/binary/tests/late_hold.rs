use std::cell::Cell;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_fleet::{
    Error, FleetClient,
    diff::{DiffMode, DiffRead, DiffSide},
};
use subspace_framework::client_test_support::Mock;

use super::{DiffView, PATH, Position, assert_page, binary_response, click, mount};
use crate::diff::view::{
    DiffSource, binary::read::Blob, crew_tests::ReadGate, source::FleetSource,
};

fn capture(
    view: &Entity<DiffView>,
    window: &mut VisualTestContext,
) -> (FleetSource, FleetClient, DiffMode, DiffRead) {
    view.read_with(window, |view, cx| {
        let DiffSource::Fleet(source) = &view.source else {
            panic!("expected Fleet diff");
        };
        (
            source.clone(),
            source.client(cx).unwrap(),
            view.fleet.mode,
            view.binary.as_ref().unwrap().closed.clone(),
        )
    })
}

fn worker(
    (source, client, mode, read): (FleetSource, FleetClient, DiffMode, DiffRead),
    position: Position,
) -> std::thread::JoinHandle<Result<(), Error>> {
    std::thread::spawn(move || {
        Blob {
            path: PATH,
            at: "B",
            side: DiffSide::New,
        }
        .read(&client, source.request(mode), &position, &read)
        .map(|_| ())
    })
}

#[gpui_kit::test]
fn a_reopened_reader_does_not_retry_under_a_late_hold(cx: &mut TestAppContext) {
    // The gated HTTP worker can wake the visual executor from its own thread.
    cx.executor().allow_parking();
    let (first_gate, first_waiting, release_first) = ReadGate::new();
    let (second_gate, second_waiting, release_second) = ReadGate::new();
    let first_gate = Mutex::new(Some(first_gate));
    let second_gate = Mutex::new(Some(second_gate));
    let starts = AtomicUsize::new(0);
    let (retried, retry_seen) = mpsc::channel();
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("/diff/file?") {
            if request.path.contains("&page=png-tail") {
                if let Some(gate) = second_gate.lock().unwrap().take() {
                    gate.pause();
                    return (503, vec![("Retry-After".into(), "0".into())], String::new());
                }
                retried.send(()).unwrap();
            } else if starts.fetch_add(1, Ordering::Relaxed) == 1 {
                first_gate.lock().unwrap().take().unwrap().pause();
                return (503, vec![("Retry-After".into(), "9".into())], String::new());
            }
        }
        let (status, body) = binary_response(request);
        (status, vec![], body)
    });
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    let first = worker(capture(&view, window), Position::default());
    first_waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    click(window, "crew-binary-close");
    click(window, ("crew-diff-bytes", 0usize));
    assert_page(&view, window, DiffSide::New, 0);
    let position = Position {
        cursor: Some("png-tail".into()),
        offset: 3,
    };
    let second = worker(capture(&view, window), position);
    second_waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    release_first.send(()).unwrap();
    assert_eq!(first.join().unwrap(), Err(Error::DiffReadCancelled));
    view.read_with(window, |view, cx| {
        assert_eq!(view.binary_retry_remaining(cx), 9)
    });
    release_second.send(()).unwrap();
    let early = retry_seen.recv_timeout(Duration::from_millis(500));
    click(window, "crew-binary-close");
    let result = second.join().unwrap();
    assert_ne!(
        early,
        Ok(()),
        "new reader retried during a recorded late hold"
    );
    assert_eq!(result, Err(Error::DiffReadCancelled));
}

#[gpui_kit::test]
fn a_late_hold_reaches_the_loaded_replacement_reader(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (gate, waiting, release) = ReadGate::new();
    let gate = Mutex::new(Some(gate));
    let reads = AtomicUsize::new(0);
    let mock = Mock::with_headers(move |request| {
        if request.path.contains("/diff/file?") && reads.fetch_add(1, Ordering::Relaxed) == 1 {
            gate.lock().unwrap().take().unwrap().pause();
            return (
                503,
                vec![("Retry-After".into(), "10".into())],
                String::new(),
            );
        }
        let (status, body) = binary_response(request);
        (status, vec![], body)
    });
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    let first = worker(capture(&view, window), Position::default());
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    click(window, "crew-binary-close");
    click(window, ("crew-diff-bytes", 0usize));
    assert_page(&view, window, DiffSide::New, 0);
    window.update(|window, _| assert!(window.find("crew-binary-preview").visible()));

    let notified = Rc::new(Cell::new(0));
    let count = notified.clone();
    let _subscription =
        window.update(|_, cx| cx.observe(&view, move |_, _| count.set(count.get() + 1)));
    release.send(()).unwrap();
    assert_eq!(first.join().unwrap(), Err(Error::DiffReadCancelled));
    window.run_until_parked();
    assert!(
        notified.get() > 0,
        "late hold did not reach the loaded reader"
    );
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(window.find("crew-binary-hold").visible());
        assert_eq!(window.find("crew-binary-preview").disabled(), Some(true));
    });
    view.read_with(window, |view, cx| {
        assert_eq!(view.binary_retry_remaining(cx), 10)
    });
    let before = mock.requests().len();
    click(window, "crew-binary-preview");
    window.executor().advance_clock(Duration::from_secs(9));
    window.run_until_parked();
    click(window, "crew-binary-preview");
    assert_eq!(mock.requests().len(), before, "preview sent before expiry");

    window.executor().advance_clock(Duration::from_secs(1));
    window.run_until_parked();
    window.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(window.try_find("crew-binary-hold").is_none());
        assert_ne!(window.find("crew-binary-preview").disabled(), Some(true));
    });
    click(window, "crew-binary-preview");
    view.read_with(window, |view, _| {
        assert!(view.binary.as_ref().unwrap().preview.is_some())
    });
    assert_eq!(
        mock.requests().len() - before,
        2,
        "one preview, in two pages"
    );
}
