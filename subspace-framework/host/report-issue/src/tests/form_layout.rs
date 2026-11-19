use super::support::{client, field, input, setup};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ScrollDelta, TestAppContext, point, px, size};

#[gpui_kit::test]
fn steps_reserve_ten_native_text_rows_and_scroll_long_content(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let steps = report.read(cx).form.steps.clone();
        let line_height = steps.read(cx).line_height().unwrap();
        let bounds = window
            .find(field(&report, "Steps to reproduce (optional)", cx))
            .bounds();
        assert!(
            bounds.size.height >= line_height * 10.,
            "steps must show at least ten rows: {bounds:?}"
        );
        steps.update(cx, |input, cx| {
            input.set_value(
                (1..=30)
                    .map(|row| format!("Step {row}\n"))
                    .collect::<String>(),
                window,
                cx,
            );
        });
        window.render_frame(cx);
        window.scroll(
            field(&report, "Steps to reproduce (optional)", cx),
            ScrollDelta::Pixels(point(px(0.), -line_height * 20.)),
            cx,
        );
        assert!(steps.read(cx).scroll_offset().y < px(0.));
        assert!(steps.read(cx).visible_row_range().unwrap().start > 0);
        assert_eq!(
            window
                .find(field(&report, "Steps to reproduce (optional)", cx))
                .bounds()
                .size
                .height,
            bounds.size.height
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn automatic_diagnostics_are_collapsed_and_keyboard_disclosable(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("report-diagnostics-toggle").expanded(),
            Some(false)
        );
        assert_eq!(
            window.find("report-diagnostics-toggle").label(),
            Some("Automatic diagnostics, 8 fields")
        );
        assert!(window.try_find("report-diagnostics-grid").is_none());
        let revision = report.read(cx).state.revision;
        window.click("report-diagnostics-toggle", cx);
        assert_eq!(
            window.find("report-diagnostics-toggle").expanded(),
            Some(true)
        );
        assert!(window.find("report-diagnostics-grid").visible());
        window.press("space", cx);
        assert_eq!(
            window.find("report-diagnostics-toggle").expanded(),
            Some(false)
        );
        window.press("enter", cx);
        assert_eq!(
            window.find("report-diagnostics-toggle").expanded(),
            Some(true)
        );
        assert_eq!(
            report.read(cx).state.revision,
            revision,
            "disclosure cannot change report consent or invalidate text"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_short_window_can_scroll_to_the_form_commit(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        input(
            window,
            &report,
            "Description (required)",
            "Small window fixture",
            cx,
        );
    })
    .unwrap();
    cx.simulate_window_resize(handle, size(px(720.), px(600.)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("report-type").visible());
        assert!(!window.find("report-review").visible());
        window.scroll(
            "report-type",
            ScrollDelta::Pixels(point(px(0.), px(-2000.))),
            cx,
        );
        assert!(window.find("report-review").visible());
        assert!(window.find("report-review").bounds().bottom() <= px(600.));
        window.click("report-review", cx);
        assert!(matches!(
            report.read(cx).state.phase,
            crate::report::Phase::Preparing
        ));
    })
    .unwrap();
}
