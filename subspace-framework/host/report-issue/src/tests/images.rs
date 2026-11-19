use super::support::{client, setup};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardEntry, TestAppContext};

#[gpui_kit::test]
fn picker_cancellation_and_invalid_content_preserve_form(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        window.click("report-choose-image", cx)
    })
    .unwrap();
    assert!(
        cx.did_prompt_for_paths(),
        "first activation must open a picker"
    );
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update(|cx| assert!(!report.read(cx).state.busy()));
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("synthetic-invalid.png");
    std::fs::write(&file, b"not a PNG").unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click("report-choose-image", cx)
    })
    .unwrap();
    assert!(
        cx.did_prompt_for_paths(),
        "second activation after cancellation must open a picker"
    );
    cx.simulate_path_prompt_response(|_| Some(vec![file]));
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(report.read(cx).state.attachments.len(), 0);
        assert_eq!(
            report.read(cx).state.error.as_deref(),
            Some("Choose a PNG or JPEG image.")
        );
    });
}

#[gpui_kit::test]
fn explicit_picker_copy_save_and_remove_use_memory_pixels(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("synthetic-pixels.png");
    image::RgbImage::from_pixel(2, 3, image::Rgb([20, 80, 160]))
        .save(&file)
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click("report-choose-image", cx)
    })
    .unwrap();
    cx.simulate_path_prompt_response(|_| Some(vec![file]));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(("report-remove-image", 0usize)).visible());
        window.click("report-copy-image", cx);
        assert!(matches!(
            cx.read_from_clipboard().unwrap().entries(),
            [ClipboardEntry::Image(_)]
        ));
        window.click("report-save-image", cx);
    })
    .unwrap();
    let destination = tempfile::tempdir().unwrap();
    let saved = destination.path().join("report-image-1.png");
    cx.simulate_path_prompt_response(|_| Some(vec![destination.path().to_path_buf()]));
    cx.run_until_parked();
    assert!(image::open(saved).is_ok());
    cx.update_window(handle, |_, window, cx| {
        window.click(("report-remove-image", 0usize), cx)
    })
    .unwrap();
    cx.update(|cx| assert_eq!(report.read(cx).state.attachments.len(), 0));
}
