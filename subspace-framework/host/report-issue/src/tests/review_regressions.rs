use super::fallback::image;
use super::support::{client, field, input, reviewed, send, setup};
use crate::report::Phase;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardEntry, TestAppContext};
use subspace_client_test_support::Mock;

#[gpui_kit::test]
fn browser_result_recovers_exact_title_and_body_after_image_copy_and_reopen(
    cx: &mut TestAppContext,
) {
    let server = Mock::new(|_| panic!("Browser recovery must not use HTTP"));
    let (handle, report) = reviewed(cx, client(server.base_url(), false), &"界".repeat(1_000));
    let expected = cx.update(|cx| {
        report.update(cx, |report, _| {
            report.state.attachments.append(image()).unwrap();
        });
        let Phase::Review(plan) = &report.read(cx).state.phase else {
            panic!("preview")
        };
        (plan.title().to_owned(), plan.body().to_owned())
    });
    send(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(field(&report, "Exact title", cx)).value(),
            Some(expected.0.as_str())
        );
        assert_eq!(
            window.find(field(&report, "Exact issue body", cx)).value(),
            Some(expected.1.as_str())
        );
        window.click("report-body-pasted", cx);
        window.click("report-copy-image", cx);
        assert!(matches!(
            cx.read_from_clipboard().unwrap().entries(),
            [ClipboardEntry::Image(_)]
        ));
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("open-report", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.click("report-copy-body", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(expected.1.as_str())
        );
        window.click("report-copy-title", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(expected.0.as_str())
        );
        window.click("report-copy-body", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(expected.1.as_str())
        );
        assert!(matches!(report.read(cx).state.phase, Phase::Browser(_)));
        assert!(window.try_find("report-submit").is_none());
    })
    .unwrap();
    assert!(server.requests().is_empty());
}

#[gpui_kit::test]
fn escape_during_preparation_keeps_plan_without_stealing_restored_focus(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        input(
            window,
            &report,
            "Description (required)",
            "Keep this prepared draft",
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.click("report-review", cx);
        assert!(matches!(report.read(cx).state.phase, Phase::Preparing));
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!report.read(cx).state.open);
        assert!(matches!(report.read(cx).state.phase, Phase::Review(_)));
        assert_eq!(window.find("open-report").focused(), Some(true));
        assert!(window.try_find(field(&report, "Exact title", cx)).is_none());
    })
    .unwrap();
}
