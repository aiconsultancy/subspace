use super::support::pending_report;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardEntry, TestAppContext};

#[gpui_kit::test]
fn image_copy_waits_for_body_acknowledgement_and_copy_body_rearms_guard(cx: &mut TestAppContext) {
    let (handle, report, body) = pending_report(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("report-copy-image", cx);
        report.update(cx, |report, cx| report.copy_image(cx));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(body.as_str()),
            "image copy must not replace a body awaiting paste"
        );
        assert_eq!(
            window.find("report-copy-image").label(),
            Some("Copy image 1 of 2")
        );
        window.click("report-body-pasted", cx);
        window.click("report-copy-image", cx);
        assert!(matches!(
            cx.read_from_clipboard().unwrap().entries(),
            [ClipboardEntry::Image(_)]
        ));
        assert_eq!(
            window.find("report-copy-image").label(),
            Some("Copy image 2 of 2")
        );
        window.click("report-copy-body", cx);
        window.click("report-copy-image", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(body.as_str())
        );
        assert_eq!(
            window.find("report-copy-image").label(),
            Some("Copy image 2 of 2")
        );
        window.click("report-body-pasted", cx);
        window.click("report-copy-image", cx);
        assert!(matches!(
            cx.read_from_clipboard().unwrap().entries(),
            [ClipboardEntry::Image(_)]
        ));
        assert_eq!(
            window.find("report-copy-image").label(),
            Some("Copy image 1 of 2")
        );
    })
    .unwrap();
}
