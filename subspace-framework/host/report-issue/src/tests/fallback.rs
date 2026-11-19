use super::support::{client, diagnostics, field, input, review, reviewed, send, setup};
use crate::{Attachment, report::Phase};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardEntry, TestAppContext};
use subspace_client_github::issue_report::{BrowserMode, TransportKind};
use subspace_client_test_support::Mock;

pub(super) fn image() -> Attachment {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 3, image::Rgb([20, 80, 160])))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    Attachment::from_bytes(bytes.into_inner()).unwrap()
}

#[gpui_kit::test]
fn clipboard_required_fallback_preserves_body_until_explicit_image_copy(cx: &mut TestAppContext) {
    let server = Mock::new(|_| panic!("No-auth fallback must not use HTTP"));
    let (handle, report) = reviewed(cx, client(server.base_url(), false), &"界".repeat(1_000));
    let expected = cx.update(|cx| {
        report.update(cx, |report, _| {
            report.state.attachments.append(image()).unwrap();
        });
        let report = report.read(cx);
        let Phase::Review(plan) = &report.state.phase else {
            panic!("preview")
        };
        assert_eq!(plan.transport(), TransportKind::Browser);
        assert_eq!(
            plan.browser_fallback().editor_mode(),
            BrowserMode::ClipboardRequired
        );
        plan.body().to_owned()
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(field(&report, "Exact issue body", cx)).value(),
            Some(expected.as_str())
        );
        assert!(
            cx.read_from_clipboard().is_none(),
            "review does not overwrite clipboard"
        );
    })
    .unwrap();
    send(cx, handle);
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some(expected.as_str())
    );
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(matches!(report.read(cx).state.phase, Phase::Browser(_)));
        assert!(
            report
                .read(cx)
                .state
                .notice
                .as_deref()
                .unwrap()
                .contains("Paste it into GitHub first")
        );
        window.click("report-body-pasted", cx);
        window.click("report-copy-image", cx);
        assert!(matches!(
            cx.read_from_clipboard().unwrap().entries(),
            [ClipboardEntry::Image(_)]
        ));
    })
    .unwrap();
    assert!(server.requests().is_empty());
}

#[gpui_kit::test]
fn preview_shows_effective_diagnostic_trimming_and_freezes_reviewed_diagnostics(
    cx: &mut TestAppContext,
) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_diagnostics(diagnostics().with_build("dev", "a".repeat(128)), cx)
        });
        input(
            window,
            &report,
            "Description (required)",
            &"界".repeat(840),
            cx,
        );
        input(window, &report, "Title (optional)", "a", cx);
    })
    .unwrap();
    cx.run_until_parked();
    review(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_diagnostics(diagnostics().with_active_board("github"), cx)
        });
        let Phase::Review(plan) = &report.read(cx).state.phase else {
            panic!("preview")
        };
        assert!(plan.browser_fallback().diagnostics_omitted() > 0);
        assert_ne!(plan.body(), plan.report().body());
        let expected = plan.body().to_owned();
        window.render_frame(cx);
        assert_eq!(
            window.find(field(&report, "Exact issue body", cx)).value(),
            Some(expected.as_str())
        );
        assert_eq!(
            report.read(cx).diagnostics.active_board(),
            None,
            "diagnostics cannot change a reviewed plan"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn field_change_events_invalidate_a_prepared_plan(cx: &mut TestAppContext) {
    let (handle, report) = reviewed(
        cx,
        client("http://127.0.0.1:1", false),
        "Original description",
    );
    for changed in ["title", "description", "steps", "type"] {
        cx.update_window(handle, |_, window, cx| {
            report.update(cx, |report, cx| match changed {
                "title" => report.form.title.update(cx, |state, cx| {
                    state.set_value("Replacement title", window, cx);
                    cx.emit(gpui_kit::component::input::InputEvent::Change);
                }),
                "description" => report.form.description.update(cx, |state, cx| {
                    state.set_value("Replacement description", window, cx);
                    cx.emit(gpui_kit::component::input::InputEvent::Change);
                }),
                "steps" => report.form.steps.update(cx, |state, cx| {
                    state.set_value("Replacement steps", window, cx);
                    cx.emit(gpui_kit::component::input::InputEvent::Change);
                }),
                _ => report.form.kind.update(cx, |state, cx| {
                    state.set_selected_value(&"Other".to_owned(), window, cx);
                    cx.emit(gpui_kit::component::select::SelectEvent::Confirm(Some(
                        "Other".to_owned(),
                    )));
                }),
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(
                matches!(report.read(cx).state.phase, Phase::Editing),
                "{changed} must invalidate review"
            )
        });
        review(cx, handle);
    }
    cx.update_window(handle, |_, _, cx| {
        report.update(cx, |report, cx| {
            report.state.attachments.append(image()).unwrap();
            report.remove_image(0, cx);
            assert!(matches!(report.state.phase, Phase::Editing));
        })
    })
    .unwrap();
}
