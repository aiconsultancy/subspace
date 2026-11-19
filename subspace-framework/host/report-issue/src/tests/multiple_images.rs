use super::support::{client, input, review, send, setup};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, ClipboardEntry, ClipboardItem, TestAppContext};
use subspace_client_test_support::Mock;

pub(super) fn pick(
    cx: &mut TestAppContext,
    handle: AnyWindowHandle,
    paths: Vec<std::path::PathBuf>,
) {
    cx.update_window(handle, |_, window, cx| {
        window.click("report-choose-image", cx)
    })
    .unwrap();
    cx.simulate_path_prompt_response(|_| Some(paths));
    cx.run_until_parked();
}

pub(super) fn pixel_file(
    directory: &std::path::Path,
    name: &str,
    pixel: [u8; 3],
) -> std::path::PathBuf {
    let path = directory.join(name);
    image::RgbImage::from_pixel(2, 3, image::Rgb(pixel))
        .save(&path)
        .unwrap();
    path
}

pub(super) fn pick_two(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    let directory = tempfile::tempdir().unwrap();
    pick(
        cx,
        handle,
        vec![
            pixel_file(directory.path(), "first.png", [20, 80, 160]),
            pixel_file(directory.path(), "second.png", [160, 80, 20]),
        ],
    );
}

#[gpui_kit::test]
fn multiple_selection_keeps_five_and_visibly_rejects_the_sixth(cx: &mut TestAppContext) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    let directory = tempfile::tempdir().unwrap();
    let paths = (0..6)
        .map(|ix| {
            pixel_file(
                directory.path(),
                &format!("synthetic-image-{ix}.png"),
                [20, 80, 160],
            )
        })
        .collect::<Vec<_>>();
    pick(cx, handle, paths);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("report-image-count").label(),
            Some("5 of 5 images")
        );
        window.click("report-choose-image", cx);
    })
    .unwrap();
    assert!(
        !cx.did_prompt_for_paths(),
        "the full collection disables the picker"
    );
    cx.update_window(handle, |_, window, cx| {
        assert!(
            report
                .read(cx)
                .state
                .error
                .as_deref()
                .unwrap()
                .contains("up to 5 images")
        );
        assert!(window.find(("report-remove-image", 0usize)).visible());
        assert!(window.find(("report-remove-image", 4usize)).visible());
        assert!(window.try_find(("report-remove-image", 5usize)).is_none());
        window.click(("report-remove-image", 1usize), cx);
        assert_eq!(
            window.find("report-image-count").label(),
            Some("4 of 5 images")
        );
        assert!(window.try_find(("report-remove-image", 1usize)).is_none());
        assert!(
            window.find(("report-remove-image", 4usize)).visible(),
            "remaining image identities cannot shift"
        );
        window.click("report-choose-image", cx);
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths(), "removal enables selection again");
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
}

#[gpui_kit::test]
fn created_report_waits_for_explicit_ordered_image_copy(cx: &mut TestAppContext) {
    let server = Mock::new(|request| {
        if request.method == "POST" {
            (
                201,
                r#"{"number":42,"html_url":"https://github.com/example-org/example-app/issues/42"}"#
                    .into(),
            )
        } else {
            (404, "{}".into())
        }
    });
    let (handle, report) = setup(cx, client(server.base_url(), true));
    pick_two(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        input(
            window,
            &report,
            "Description (required)",
            "Synthetic multi-image report",
            cx,
        );
        cx.write_to_clipboard(ClipboardItem::new_string("pending body sentinel".into()));
    })
    .unwrap();
    review(cx, handle);
    send(cx, handle);
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().as_deref(),
        Some("pending body sentinel")
    );
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("report-copy-image").label(),
            Some("Copy image 1 of 2")
        );
        assert_copy_sequence(window, cx);
        assert!(window.try_find("report-submit").is_none());
    })
    .unwrap();
    let posts = server
        .requests()
        .into_iter()
        .filter(|request| request.method == "POST")
        .collect::<Vec<_>>();
    assert_eq!(posts.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&posts[0].body).unwrap();
    assert_eq!(
        payload.as_object().unwrap().len(),
        2,
        "REST sends only the reviewed title and body, never image bytes or filenames"
    );
    assert!(!posts[0].body.contains("first.png"));
}

fn assert_copy_sequence(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    for (next_label, expected) in [
        ("Copy image 2 of 2", [20, 80, 160]),
        ("Copy image 1 of 2", [160, 80, 20]),
    ] {
        window.click("report-copy-image", cx);
        let clipboard = cx.read_from_clipboard().unwrap();
        let [ClipboardEntry::Image(image)] = clipboard.entries() else {
            panic!("validated image clipboard entry")
        };
        assert_eq!(
            image::load_from_memory(&image.bytes)
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            expected
        );
        assert_eq!(window.find("report-copy-image").label(), Some(next_label));
    }
}
