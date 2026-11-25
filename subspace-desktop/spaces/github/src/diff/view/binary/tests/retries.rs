use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_fleet::{Error, diff::DiffSide};
use subspace_framework::client_test_support::Mock;

use super::{PATH, assert_page, binary_response, click, mount};
use crate::diff::view::binary::{
    preview::{PreviewError, PreviewRead},
    read::Blob,
};
use crate::diff::view::{DiffSource, crew_tests::ReadGate};

#[gpui_kit::test]
fn a_live_change_cancels_a_preview_between_pages(cx: &mut TestAppContext) {
    let mock = Mock::new(binary_response);
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    let (source, client, mode, read) = view.read_with(window, |view, cx| {
        let DiffSource::Fleet(source) = &view.source else {
            panic!("expected Fleet diff")
        };
        (
            source.clone(),
            source.client(cx).unwrap(),
            view.fleet.mode,
            view.binary.as_ref().unwrap().closed.clone(),
        )
    });
    let (gate, waiting, release) = ReadGate::new();
    let worker = std::thread::spawn(move || {
        let blob = Blob {
            path: PATH,
            at: "B",
            side: DiffSide::New,
        };
        PreviewRead::default().read(|position| {
            let page = blob
                .read(&client, source.request(mode), position, &read)
                .map_err(PreviewError::Fleet)?;
            if position.offset == 0 {
                gate.pause();
            }
            Ok(page)
        })
    });
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    view.update(window, |view, _| {
        assert!(view.observe_crew_fingerprint(Some("C".into())))
    });
    release.send(()).unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Err(PreviewError::Fleet(Error::DiffReadCancelled))
    ));
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.contains("&page=png-tail"))
    );
    assert_page(&view, window, DiffSide::New, 0);
}

#[gpui_kit::test]
fn retry_page_resumes_a_busy_preview_without_fetching_its_prefix_again(cx: &mut TestAppContext) {
    let mock = busy_tail();
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    click(window, "crew-binary-preview");
    assert_page(&view, window, DiffSide::New, 0);
    window.update(|window, _| assert!(window.find("crew-binary-retry-page").visible()));
    click(window, "crew-binary-retry-page");
    view.read_with(window, |view, _| {
        let binary = view.binary.as_ref().unwrap();
        assert!(binary.preview.is_some());
        assert!(binary.retry.is_none());
    });
    assert_tail_was_retried(&mock, 4);
}

#[gpui_kit::test]
fn retry_page_keeps_the_current_bytes_until_the_failed_page_arrives(cx: &mut TestAppContext) {
    let mock = busy_tail();
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    click(window, "crew-binary-next");
    assert_page(&view, window, DiffSide::New, 0);
    window.update(|window, _| {
        let next = window.find("crew-binary-next");
        assert!(next.visible());
        assert_eq!(next.disabled(), Some(true));
    });
    click(window, "crew-binary-retry-page");
    assert_page(&view, window, DiffSide::New, 3);
    assert_tail_was_retried(&mock, 3);
    click(window, "crew-binary-previous");
    assert_page(&view, window, DiffSide::New, 0);
}

fn busy_tail() -> Mock {
    let tail = AtomicUsize::new(0);
    Mock::with_headers(move |request| {
        if request.path.contains("&page=png-tail") && tail.fetch_add(1, Ordering::Relaxed) == 0 {
            return (
                503,
                vec![("Retry-After".into(), "11".into())],
                String::new(),
            );
        }
        let (status, body) = binary_response(request);
        (status, vec![], body)
    })
}

fn assert_tail_was_retried(mock: &Mock, file_count: usize) {
    let requests = mock.requests();
    let paths: Vec<_> = requests
        .iter()
        .filter(|req| req.path.contains("/diff/file?"))
        .map(|req| req.path.as_str())
        .collect();
    assert_eq!(paths.len(), file_count);
    let tail = &paths[file_count - 2..];
    assert!(tail[0].contains("&page=png-tail"));
    assert_eq!(tail[0], tail[1]);
    assert_eq!(
        requests
            .iter()
            .filter(|req| req.path.contains("/diff?"))
            .count(),
        1
    );
}
