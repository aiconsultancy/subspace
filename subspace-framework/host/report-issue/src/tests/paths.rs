use super::support::{client, destination, diagnostics, input, setup};
use crate::{ReportIssue, form::diagnostic_text};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Role, TestAppContext};
use subspace_core::diagnostics::{
    IssuePaths, LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails,
};

const LEGACY_DIAGNOSTICS: &str = concat!(
    "Product: subspace\n",
    "App version: 1.15.0\n",
    "Build channel: dev\n",
    "Commit: fixture\n",
    "OS: macos\n",
    "Architecture: aarch64\n",
    "Theme: one-dark\n",
    "Icon size: medium",
);

#[test]
fn off_diagnostics_keep_the_legacy_form_bytes() {
    assert_eq!(diagnostic_text(&diagnostics()), LEGACY_DIAGNOSTICS);
}

pub(super) fn synthetic_paths() -> IssuePaths {
    LocalDiagnostics::default()
        .with_runtime(RuntimeDetails::new(
            "/Users/fixture/Applications/Subspace.app/Contents/MacOS/subspace",
            LaunchKind::Desktop,
            "login shell /Users/fixture/bin/zsh",
            Some("/Users/fixture/bin/az".into()),
        ))
        .with_redaction(RedactionContext::new(Some("/Users/fixture".into()), []))
        .issue_paths(Some("/Users/fixture/config/fixture-tunnel.json"))
}

#[gpui_kit::test]
fn incoming_diagnostics_cannot_enable_paths_without_consent(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update(|cx| {
        report.update(cx, |report, cx| {
            report.set_diagnostics(diagnostics().with_paths(synthetic_paths()), cx);
            assert_eq!(diagnostic_text(&report.diagnostics), LEGACY_DIAGNOSTICS);
            assert!(report.diagnostics.paths().is_none());
        })
    });
    cx.update_window(handle, |_, window, cx| {
        let fresh = cx.new(|cx| {
            ReportIssue::new(
                destination(client("http://127.0.0.1:1", false)),
                diagnostics().with_paths(synthetic_paths()),
                ".".into(),
                window,
                cx,
            )
        });
        assert!(fresh.read(cx).diagnostics.paths().is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn include_paths_is_a_native_off_by_default_form_switch(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let switch = window.find("report-include-paths");
        assert_eq!(switch.role(), Some(Role::Switch));
        assert_eq!(switch.label(), Some("Include paths"));
        assert_eq!(switch.checked(), Some(false));
        input(
            window,
            &report,
            "Description (required)",
            "Fixture draft",
            cx,
        );
    })
    .unwrap();
}
