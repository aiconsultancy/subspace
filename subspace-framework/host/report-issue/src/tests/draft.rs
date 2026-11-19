use super::support::{client, diagnostics, field, input, repository, review, reviewed, setup};
use crate::{Draft, prepare, report::Phase};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Focusable as _, TestAppContext};

#[gpui_kit::test]
fn validates_required_description_and_keeps_unknown_type(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            window
                .find(field(&report, "Description (required)", cx))
                .focused(),
            Some(true)
        );
        window.click("report-review", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let report = report.read(cx);
        assert!(
            report
                .state
                .error
                .as_deref()
                .unwrap()
                .contains("Enter a description")
        );
        assert!(matches!(report.state.phase, Phase::Editing));
        assert_eq!(
            report.form.draft(cx).issue_type(),
            subspace_client_github::issue_report::IssueType::Unknown
        );
    });
}

#[gpui_kit::test]
fn unknown_type_and_exact_readonly_preview(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        input(
            window,
            &report,
            "Description (required)",
            "  first line  \n**exact** text",
            cx,
        )
    })
    .unwrap();
    review(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let expected = prepare(
            &Draft::new("  first line  \n**exact** text"),
            &diagnostics(),
            &repository(),
        )
        .unwrap();
        assert_eq!(
            window.find(field(&report, "Exact title", cx)).value(),
            Some("first line")
        );
        assert_eq!(
            window.find(field(&report, "Exact issue body", cx)).value(),
            Some(expected.body())
        );
        window.input("must not change readonly title", cx);
        assert_eq!(
            window.find(field(&report, "Exact title", cx)).value(),
            Some("first line")
        );
    })
    .unwrap();
    cx.update(|cx| assert!(matches!(report.read(cx).state.phase, Phase::Review(_))));
}

#[gpui_kit::test]
fn escape_reopen_retains_draft_and_restores_per_open_focus(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        input(
            window,
            &report,
            "Description (required)",
            "Unsent draft",
            cx,
        );
        input(window, &report, "Title (optional)", "Retained title", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("report-review").is_none());
        assert_eq!(window.find("open-report").focused(), Some(true));
        window.click("open-report", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(field(&report, "Description (required)", cx))
                .value(),
            Some("Unsent draft")
        );
        assert_eq!(
            window.find(field(&report, "Title (optional)", cx)).value(),
            Some("Retained title")
        );
        assert_eq!(
            window
                .find(field(&report, "Description (required)", cx))
                .focused(),
            Some(true)
        );
        report.update(cx, |report, cx| {
            report.set_diagnostics(diagnostics().with_active_board("github"), cx);
            assert_eq!(report.diagnostics.active_board(), Some("github"));
            assert!(report.form.description.focus_handle(cx).is_focused(window));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn edit_invalidates_review_and_dropdown_changes_the_report(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        input(window, &report, "Description (required)", "Description", cx);
        window.click("report-type", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    review(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.click("report-edit", cx);
        input(window, &report, "Title (optional)", "Changed title", cx);
        input(
            window,
            &report,
            "Steps to reproduce (optional)",
            "Exact steps",
            cx,
        );
        assert!(window.try_find("report-submit").is_none());
    })
    .unwrap();
    cx.run_until_parked();
    review(cx, handle);
    cx.update(|cx| {
        let report = report.read(cx);
        let Phase::Review(plan) = &report.state.phase else {
            panic!("review required")
        };
        assert_eq!(plan.title(), "Changed title");
        assert!(plan.body().contains("Exact steps"));
        assert_ne!(
            plan.report().issue_type(),
            subspace_client_github::issue_report::IssueType::Unknown
        );
    });
}

#[gpui_kit::test]
fn reopening_review_keeps_plan_and_focus_without_enter_dismissal(cx: &mut TestAppContext) {
    let (handle, report) = reviewed(
        cx,
        client("http://127.0.0.1:1", false),
        "Retained reviewed report",
    );
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("open-report", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(field(&report, "Exact title", cx)).focused(),
            Some(true)
        );
        window.press("enter", cx);
        assert!(window.find("report-submit").visible());
        assert!(matches!(report.read(cx).state.phase, Phase::Review(_)));
    })
    .unwrap();
}

#[gpui_kit::test]
fn keyboard_order_and_control_alignment_follow_the_form(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let description = window
            .find(field(&report, "Description (required)", cx))
            .bounds();
        let title = window.find("report-title").bounds();
        let kind = window.find("report-type").bounds();
        assert_eq!(description.left(), title.left());
        assert_eq!(title.left(), kind.left());
        assert!(description.bottom() <= title.top());
        window.press("tab", cx);
        assert_eq!(window.find("report-title").focused(), Some(true));
        window.press("tab", cx);
        assert!(
            report
                .read(cx)
                .form
                .kind
                .focus_handle(cx)
                .contains_focused(window, cx)
        );
        window.press("shift-tab", cx);
        assert_eq!(window.find("report-title").focused(), Some(true));
        assert_ne!(window.find("open-report").focused(), Some(true));
    })
    .unwrap();
}
