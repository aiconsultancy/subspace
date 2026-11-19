use super::paths::synthetic_paths;
use super::support::{client, diagnostics, field, repository, setup};
use crate::{Attachment, Draft, ReportIssue, prepare, report::Phase};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardItem, TestAppContext};
use subspace_client_github::issue_report::{BrowserMode, ErrorKind};

fn assert_path_lines(body: &str) {
    for line in [
        "- Executable: ~/Applications/Subspace.app/Contents/MacOS/subspace",
        "- Launch: desktop",
        "- PATH source: login shell ~/bin/zsh",
        "- Azure CLI: ~/bin/az",
        "- Configuration file: ~/config/fixture-tunnel.json",
    ] {
        assert!(body.contains(line), "missing {line}: {body}");
    }
}

#[gpui_kit::test]
fn path_preview_fixture_prepares_path_rows_before_the_first_presented_scene(
    cx: &mut TestAppContext,
) {
    let client = client("http://127.0.0.1:1", false);
    let (handle, report) = setup(cx, client.clone());
    let draft = Draft::new("The synthetic preview does not resize\nFixture data only.")
        .with_steps("1. Open a fixture\n2. Resize its panel");
    let plan = client.prepare_issue_report(
        prepare(
            &draft,
            &diagnostics().with_paths(synthetic_paths()),
            &repository(),
        )
        .unwrap(),
    );
    let expected = plan.body().to_owned();
    assert_path_lines(&expected);
    let paths_start = expected
        .lines()
        .position(|line| line.starts_with("- Executable:"))
        .unwrap();
    let line_count = expected.lines().count();
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.fixture_draft(&draft, window, cx);
            report.fixture_paths(synthetic_paths(), true, cx);
            report.fixture_preview(plan, window, cx);
        });
        ReportIssue::fixture_preview_end(&report, window, cx);
        window.render_frame(cx);
        let preview = report.read(cx).form.preview_body.read(cx);
        assert_eq!(preview.value().as_ref(), expected);
        assert_eq!(preview.cursor(), expected.len());
        let visible = preview
            .visible_row_range()
            .expect("first scene laid out the real textarea");
        assert!(
            visible.start > 0,
            "first scene still uses the top rows: {visible:?}, offset {:?}",
            preview.scroll_offset()
        );
        assert!(
            visible.start <= paths_start,
            "{visible:?} excludes path row {paths_start}"
        );
        assert!(
            visible.end >= line_count,
            "{visible:?} excludes last row {line_count}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn path_preview_fixture_scrolls_the_real_viewport_to_all_path_lines(cx: &mut TestAppContext) {
    let client = client("http://127.0.0.1:1", false);
    let (handle, report) = setup(cx, client.clone());
    let draft = Draft::new("Synthetic path preview");
    let plan = client.prepare_issue_report(
        prepare(
            &draft,
            &diagnostics().with_paths(synthetic_paths()),
            &repository(),
        )
        .unwrap(),
    );
    let line_count = plan.body().lines().count();
    let paths_start = plan
        .body()
        .lines()
        .position(|line| line.starts_with("- Executable:"))
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.fixture_draft(&draft, window, cx);
            report.fixture_paths(synthetic_paths(), true, cx);
            report.fixture_preview(plan, window, cx);
        });
        ReportIssue::fixture_preview_end(&report, window, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let preview = report.read(cx).form.preview_body.read(cx);
        assert!(
            preview
                .value()
                .contains("- Configuration file: ~/config/fixture-tunnel.json")
        );
        assert_eq!(preview.cursor(), preview.text().len());
        let visible = preview
            .visible_row_range()
            .expect("laid-out native viewport");
        assert!(
            visible.start > 0,
            "preview must not remain at its first line"
        );
        assert!(
            visible.start <= paths_start,
            "{visible:?} excludes first path line {paths_start}"
        );
        assert!(
            visible.end >= line_count,
            "{visible:?} excludes final line {line_count}"
        );
    })
    .unwrap();
    assert_eq!(cx.opened_url(), None);
}

#[gpui_kit::test]
fn fixture_seeds_render_the_real_form_preview_and_simulated_error(cx: &mut TestAppContext) {
    let client = client("http://127.0.0.1:1", false);
    let (handle, report) = setup(cx, client.clone());
    let draft = Draft::new("Synthetic native form").with_title("Synthetic title");
    let plan = client.prepare_issue_report(prepare(&draft, &diagnostics(), &repository()).unwrap());
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            report.fixture_draft(&draft, window, cx);
            report.fixture_preview(plan, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(field(&report, "Exact title", cx)).value(),
            Some("Synthetic title")
        );
        assert!(
            matches!(report.read(cx).state.phase, Phase::Review(_)),
            "deferred seed events must not cancel preview"
        );
        report.update(cx, |report, cx| {
            report.fixture_draft(&draft, window, cx);
            report.fixture_error(ErrorKind::AccessDenied.into(), cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            report
                .read(cx)
                .state
                .error
                .as_deref()
                .unwrap()
                .starts_with("Simulated outcome:")
        )
    });
}

#[gpui_kit::test]
fn fixture_attachment_accepts_only_the_validated_pixel_boundary(cx: &mut TestAppContext) {
    let (_, report) = setup(cx, client("http://127.0.0.1:1", false));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 3)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let image = Attachment::from_bytes(bytes.into_inner()).unwrap();
    cx.update(|cx| {
        report.update(cx, |report, cx| {
            report.fixture_attachment(image, cx);
            assert_eq!(
                report.state.attachments.items()[0]
                    .image
                    .preview()
                    .size(0)
                    .height
                    .0,
                3
            );
        })
    });
}

#[gpui_kit::test]
fn browser_fixture_seeds_exact_copy_controls_without_external_effects(cx: &mut TestAppContext) {
    let client = client("http://127.0.0.1:1", false);
    let (handle, report) = setup(cx, client.clone());
    let draft = Draft::new("界".repeat(1_000)).with_title("Synthetic browser title");
    let plan = client.prepare_issue_report(prepare(&draft, &diagnostics(), &repository()).unwrap());
    assert_eq!(
        plan.browser_fallback().editor_mode(),
        BrowserMode::ClipboardRequired
    );
    let expected = (plan.title().to_owned(), plan.body().to_owned());
    cx.update_window(handle, |_, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(
            "test-only-clipboard-sentinel".into(),
        ));
        report.update(cx, |report, cx| report.fixture_browser(plan, window, cx));
        window.render_frame(cx);
        assert!(matches!(report.read(cx).state.phase, Phase::Browser(_)));
        assert_eq!(
            window.find(field(&report, "Exact title", cx)).value(),
            Some(expected.0.as_str())
        );
        assert_eq!(
            window.find(field(&report, "Exact issue body", cx)).value(),
            Some(expected.1.as_str())
        );
        assert!(window.find("report-copy-title").visible());
        assert!(window.find("report-copy-body").visible());
    })
    .unwrap();
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("test-only-clipboard-sentinel")
    );
    assert_eq!(cx.opened_url(), None);
}
