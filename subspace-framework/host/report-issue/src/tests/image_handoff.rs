use super::multiple_images::pick_two;
use super::support::{client, pending_report, setup};
use crate::report::Phase;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
fn pending_browser_body_survives_multi_image_handoff_and_folder_save(cx: &mut TestAppContext) {
    let (handle, report, expected) = pending_report(cx);
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some(expected.as_str())
    );
    cx.update_window(handle, |_, window, cx| {
        window.click("report-save-image", cx)
    })
    .unwrap();
    let saved = tempfile::tempdir().unwrap();
    cx.simulate_path_prompt_response(|_| Some(vec![saved.path().to_path_buf()]));
    cx.run_until_parked();
    assert!(image::open(saved.path().join("report-image-1.png")).is_ok());
    assert!(image::open(saved.path().join("report-image-2.png")).is_ok());
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some(expected.as_str())
    );
    cx.update(|cx| {
        assert!(matches!(report.read(cx).state.phase, Phase::Browser(_)));
        assert_eq!(report.read(cx).state.attachments.len(), 2);
    });
}

#[gpui_kit::test]
fn folder_save_cancellation_and_collision_keep_images_and_never_overwrite(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    pick_two(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.click("report-save-image", cx)
    })
    .unwrap();
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|cx| assert!(!report.read(cx).state.busy()));
    let saved = tempfile::tempdir().unwrap();
    let existing = saved.path().join("report-image-2.png");
    std::fs::write(&existing, b"test-only-existing-file").unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click("report-save-image", cx)
    })
    .unwrap();
    cx.simulate_path_prompt_response(|_| Some(vec![saved.path().to_path_buf()]));
    cx.run_until_parked();
    assert!(image::open(saved.path().join("report-image-1.png")).is_ok());
    assert_eq!(std::fs::read(existing).unwrap(), b"test-only-existing-file");
    cx.update(|cx| {
        let report = report.read(cx);
        assert_eq!(report.state.attachments.len(), 2);
        assert!(!report.state.busy());
        assert!(
            report
                .state
                .error
                .as_deref()
                .unwrap()
                .contains("Saved 1 of 2 images")
        );
    });
    let retry = tempfile::tempdir().unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click("report-save-image", cx)
    })
    .unwrap();
    cx.simulate_path_prompt_response(|_| Some(vec![retry.path().to_path_buf()]));
    cx.run_until_parked();
    cx.update(|cx| {
        let report = report.read(cx);
        assert!(report.state.error.is_none());
        assert!(
            report
                .state
                .notice
                .as_deref()
                .unwrap()
                .contains("Saved 2 images")
        );
    });
    assert!(image::open(retry.path().join("report-image-2.png")).is_ok());
}
