use super::paths::synthetic_paths;
use super::support::{client, field, input, repository, review, setup};
use crate::{Draft, prepare, report::Phase};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

fn reopen(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.click("open-report", cx))
        .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn reopening_resets_path_consent_without_losing_unsent_text(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_path_diagnostics(synthetic_paths(), cx)
        });
        input(
            window,
            &report,
            "Description (required)",
            "Unsent fixture",
            cx,
        );
        input(window, &report, "Title (optional)", "Retained title", cx);
        input(
            window,
            &report,
            "Steps to reproduce (optional)",
            "Retained steps",
            cx,
        );
        window.click("report-include-paths", cx);
        assert_eq!(window.find("report-include-paths").checked(), Some(true));
    })
    .unwrap();
    reopen(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("report-include-paths").checked(), Some(false));
        window.click("report-include-paths", cx);
    })
    .unwrap();
    review(cx, handle);
    reopen(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("report-include-paths").checked(), Some(false));
        assert_eq!(
            window
                .find(field(&report, "Description (required)", cx))
                .value(),
            Some("Unsent fixture")
        );
        assert_eq!(window.find("report-title").value(), Some("Retained title"));
        assert_eq!(
            window
                .find(field(&report, "Steps to reproduce (optional)", cx))
                .value(),
            Some("Retained steps")
        );
        assert!(window.try_find("report-submit").is_none());
        assert!(matches!(report.read(cx).state.phase, Phase::Editing));
    })
    .unwrap();
}

#[gpui_kit::test]
fn changing_either_path_choice_invalidates_a_reviewed_plan(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_path_diagnostics(synthetic_paths(), cx)
        });
        input(
            window,
            &report,
            "Description (required)",
            "Fixture draft",
            cx,
        );
    })
    .unwrap();
    for included in [true, false] {
        review(cx, handle);
        cx.update_window(handle, |_, window, cx| {
            report.update(cx, |report, cx| {
                assert!(matches!(report.state.phase, Phase::Review(_)));
                let revision = report.state.revision;
                report.set_include_paths(included, cx);
                assert!(matches!(report.state.phase, Phase::Editing));
                assert_ne!(report.state.revision, revision);
                assert_eq!(report.effective_diagnostics().paths().is_some(), included);
                report.submit(window, cx);
                assert!(matches!(report.state.phase, Phase::Editing));
            });
        })
        .unwrap();
    }
    assert_eq!(cx.opened_url(), None);
}

#[gpui_kit::test]
fn path_changes_fence_prepared_outcomes_even_during_a_new_preparation(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_path_diagnostics(synthetic_paths(), cx);
            for included in [true, false] {
                let old_revision = report.state.revision;
                let old_plan = report.client.prepare_issue_report(
                    prepare(
                        &Draft::new("Old fixture"),
                        &report.effective_diagnostics(),
                        &repository(),
                    )
                    .unwrap(),
                );
                report.state.phase = Phase::Preparing;
                report.set_include_paths(included, cx);
                assert!(matches!(report.state.phase, Phase::Editing));
                report.prepared(old_plan, old_revision, window, cx);
                assert!(matches!(report.state.phase, Phase::Editing));
                let old_plan = report.client.prepare_issue_report(
                    prepare(
                        &Draft::new("Stale fixture"),
                        &report.effective_diagnostics(),
                        &repository(),
                    )
                    .unwrap(),
                );
                report.state.phase = Phase::Preparing;
                report.prepared(old_plan, old_revision, window, cx);
                assert!(matches!(report.state.phase, Phase::Preparing));
                assert!(report.form.preview_body.read(cx).value().is_empty());
            }
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn busy_switch_rejects_input_and_async_completion_uses_the_new_choice(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_path_diagnostics(synthetic_paths(), cx)
        });
        input(
            window,
            &report,
            "Description (required)",
            "Fixture draft",
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.click("report-review", cx);
        assert!(matches!(report.read(cx).state.phase, Phase::Preparing));
        window.click("report-include-paths", cx);
        assert!(!report.read(cx).state.include_paths);
        assert_eq!(window.find("report-include-paths").checked(), Some(false));
        report.update(cx, |report, cx| {
            report.set_include_paths(true, cx);
            report.review(window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let report = report.read(cx);
        let Phase::Review(plan) = &report.state.phase else {
            panic!("new review")
        };
        assert!(
            plan.body()
                .contains("- Executable: ~/Applications/Subspace.app/Contents/MacOS/subspace")
        );
    });
}

#[gpui_kit::test]
fn path_updates_cannot_replace_submission_receipts_or_uncertain_state(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update(|cx| {
        report.update(cx, |report, cx| {
            report.set_path_diagnostics(synthetic_paths(), cx);
            report.set_include_paths(true, cx);
            let fallback = prepare(
                &Draft::new("Synthetic result"),
                &report.effective_diagnostics(),
                &repository(),
            )
            .unwrap()
            .browser_fallback();
            for phase in [
                Phase::Submitting,
                Phase::Browser(fallback),
                Phase::Uncertain { opened: false },
            ] {
                report.state.phase = phase;
                let before = std::mem::discriminant(&report.state.phase);
                let revision = report.state.revision;
                report.set_include_paths(false, cx);
                report.set_path_diagnostics(synthetic_paths(), cx);
                assert_eq!(std::mem::discriminant(&report.state.phase), before);
                assert_eq!(report.state.revision, revision);
            }
            report.state.phase =
                Phase::Created("https://github.com/example-org/example-app/issues/42".into());
            report.set_include_paths(false, cx);
        })
    });
    reopen(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let Phase::Created(url) = &report.read(cx).state.phase else {
            panic!("retained receipt")
        };
        assert!(url.ends_with("/issues/42"));
        assert!(window.find("report-open-result").visible());
        assert!(window.try_find("report-submit").is_none());
    })
    .unwrap();
    assert_eq!(cx.opened_url(), None);
}
