use super::paths::synthetic_paths;
use super::support::{client, diagnostics, field, input, review, setup};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

const EXPECTED_FIELDS: [&str; 14] = [
    "- Product: subspace",
    "- App version: 1.15.0",
    "- Build channel: dev",
    "- Commit: fixture",
    "- OS: macos",
    "- Architecture: aarch64",
    "- Active board: github",
    "- Theme: one-dark",
    "- Icon size: medium",
    "- Executable: ~/Applications/Subspace.app/Contents/MacOS/subspace",
    "- Launch: desktop",
    "- PATH source: login shell ~/bin/zsh",
    "- Azure CLI: ~/bin/az",
    "- Configuration file: ~/config/fixture-tunnel.json",
];

#[gpui_kit::test]
fn disclosure_counts_actual_fields_and_preview_keeps_all_consented_values(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_diagnostics(diagnostics().with_active_board("github"), cx);
            report.set_path_diagnostics(synthetic_paths(), cx);
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("report-diagnostics-toggle").label(),
            Some("Automatic diagnostics, 9 fields")
        );
        window.click("report-include-paths", cx);
        assert_eq!(
            window.find("report-diagnostics-toggle").label(),
            Some("Automatic diagnostics, 14 fields")
        );
        assert!(window.try_find("report-diagnostics-grid").is_none());
        window.click("report-diagnostics-toggle", cx);
        assert!(window.find("report-diagnostics-grid").visible());
        window.click("report-diagnostics-toggle", cx);
        input(
            window,
            &report,
            "Description (required)",
            "Complete diagnostics fixture",
            cx,
        );
    })
    .unwrap();
    review(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let body = window
            .find(field(&report, "Exact issue body", cx))
            .value()
            .unwrap()
            .to_owned();
        for line in EXPECTED_FIELDS {
            assert!(
                body.contains(line),
                "missing consented preview field {line}"
            );
        }
        assert_eq!(
            body.lines().filter(|line| line.starts_with("- ")).count(),
            14
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn expanded_diagnostics_use_two_aligned_accessible_columns(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_diagnostics(diagnostics().with_active_board("github"), cx)
        });
        window.render_frame(cx);
        window.click("report-diagnostics-toggle", cx);
        let label_column = window.find("report-diagnostic-key-Product").bounds();
        let value_column = window.find("report-diagnostic-value-Product").bounds();
        assert!(label_column.right() < value_column.left());
        assert_eq!(label_column.top(), value_column.top());
        for (key, value) in [
            ("Product", "subspace"),
            ("Commit", "fixture"),
            ("Active board", "github"),
            ("Icon size", "medium"),
        ] {
            let label = window.find(format!("report-diagnostic-key-{key}"));
            let cell = window.find(format!("report-diagnostic-value-{key}"));
            assert_eq!(label.label(), Some(key));
            assert_eq!(cell.label(), Some(value));
            assert_eq!(label.bounds().left(), label_column.left());
            assert_eq!(cell.bounds().left(), value_column.left());
            assert_eq!(label.bounds().top(), cell.bounds().top());
        }
    })
    .unwrap();
}
