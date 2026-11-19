use super::paths::synthetic_paths;
use super::support::{client, field, input, review, setup};
use crate::{form::diagnostic_text, report::Phase};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardItem, TestAppContext};

const OFF_BODY: &str = concat!(
    "## Description\n\nFixture draft\n\n## Type\n\nUnknown\n\n## Diagnostics\n\n",
    "- Product: subspace\n- App version: 1.15.0\n- Build channel: dev\n",
    "- Commit: fixture\n- OS: macos\n- Architecture: aarch64\n",
    "- Theme: one-dark\n- Icon size: medium",
);
const PATH_LINES: &str = concat!(
    "- Executable: ~/Applications/Subspace.app/Contents/MacOS/subspace\n",
    "- Launch: desktop\n- PATH source: login shell ~/bin/zsh\n",
    "- Azure CLI: ~/bin/az\n- Configuration file: ~/config/fixture-tunnel.json",
);

#[gpui_kit::test]
fn native_pointer_and_keyboard_choices_are_in_the_exact_review(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("fixture sentinel".into()));
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
        window.click("report-include-paths", cx);
        assert_eq!(window.find("report-include-paths").checked(), Some(true));
        assert_eq!(window.find("report-include-paths").focused(), Some(true));
        window.press("space", cx);
        assert_eq!(window.find("report-include-paths").checked(), Some(false));
        window.press("space", cx);
        assert_eq!(window.find("report-include-paths").checked(), Some(true));
        assert!(
            diagnostic_text(&report.read(cx).effective_diagnostics())
                .contains("Configuration file: ~/config/fixture-tunnel.json")
        );
    })
    .unwrap();
    review(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(field(&report, "Exact issue body", cx)).value(),
            Some(format!("{OFF_BODY}\n{PATH_LINES}").as_str())
        );
        assert!(matches!(report.read(cx).state.phase, Phase::Review(_)));
        window.click("report-edit", cx);
        window.click("report-include-paths", cx);
        assert_eq!(window.find("report-include-paths").checked(), Some(false));
    })
    .unwrap();
    review(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(field(&report, "Exact issue body", cx)).value(),
            Some(OFF_BODY)
        );
    })
    .unwrap();
    assert_eq!(cx.opened_url(), None);
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("fixture sentinel")
    );
}

#[gpui_kit::test]
fn image_busy_state_disables_path_consent(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.set_path_diagnostics(synthetic_paths(), cx);
            report.state.image_busy = true;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("report-include-paths", cx);
        window.press("space", cx);
        report.update(cx, |report, cx| report.set_include_paths(true, cx));
        assert!(!report.read(cx).state.include_paths);
        assert_eq!(window.find("report-include-paths").checked(), Some(false));
    })
    .unwrap();
}
