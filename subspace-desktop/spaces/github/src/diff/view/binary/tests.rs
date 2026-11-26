use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ElementId, Empty, Entity, RenderImage, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_client_fleet::diff::DiffSide;
use subspace_framework::client_test_support::Mock;

use super::{BinaryView, DiffView, Move, Position};
use crate::diff::view::crew_tests::mount;

const PATH: &str = "icon.png";
// A one-pixel PNG; the first three bytes are in a separate page on each side.
const PNG_TAIL: &str =
    "Rw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgZGL+DwABFAEG1rmmRQAAAABJRU5ErkJggg==";

mod retries;

#[gpui_kit::test]
fn pages_both_binary_sides_and_evicts_the_preview_on_close(cx: &mut TestAppContext) {
    let mock = Mock::new(binary_response);
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    assert_page(&view, window, DiffSide::New, 0);
    click(window, "crew-binary-next");
    assert_page(&view, window, DiffSide::New, 3);
    click(window, "crew-binary-previous");
    assert_page(&view, window, DiffSide::New, 0);
    click(window, ("crew-binary-side", 0usize));
    assert_page(&view, window, DiffSide::Old, 0);
    click(window, "crew-binary-next");
    assert_page(&view, window, DiffSide::Old, 3);

    let image = open_preview(&view, window);
    click(window, "crew-binary-return");
    assert_page(&view, window, DiffSide::Old, 3);
    window.update(|window, _| assert!(!window.has_image_atlas_entry(&image)));

    let image = open_preview(&view, window);
    click(window, "crew-binary-close");
    view.read_with(window, |view, _| assert!(view.binary.is_none()));
    window.update(|window, _| assert!(!window.has_image_atlas_entry(&image)));

    click(window, ("crew-diff-bytes", 0usize));
    assert_page(&view, window, DiffSide::New, 0);
    let image = open_preview(&view, window);
    click(window, ("crew-binary-side", 0usize));
    assert_page(&view, window, DiffSide::Old, 0);
    window.update(|window, _| assert!(!window.has_image_atlas_entry(&image)));

    let image = open_preview(&view, window);
    window.update(|window, cx| {
        window.replace_root(cx, |_, _| Empty);
    });
    drop(view);
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.run_until_parked();
    window.update(|window, _| assert!(!window.has_image_atlas_entry(&image)));
}

#[gpui_kit::test]
fn a_binary_page_conflict_keeps_the_displayed_bytes_and_offers_refresh(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.path.contains("&page=png-tail") {
            (409, String::new())
        } else {
            binary_response(request)
        }
    });
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    click(window, "crew-binary-next");
    assert_page(&view, window, DiffSide::New, 0);
    window.update(|window, _| assert!(window.find("crew-binary-refresh").visible()));
}

#[test]
fn an_empty_binary_terminator_keeps_the_last_nonempty_page_visible() {
    let mut binary = BinaryView::new(0, DiffSide::New, Default::default());
    let bytes = serde_json::from_value(json!({
        "side": "new", "encoding": "base64", "offset": 0, "size": 3,
        "data": "iVBO", "nextPage": "eof",
    }))
    .unwrap();
    binary.show(bytes, Position::default(), Move::Start);
    let position = binary.destination(&Move::Next).unwrap();
    let empty = serde_json::from_value(json!({
        "side": "new", "encoding": "base64", "offset": 3, "size": 3, "data": "",
    }))
    .unwrap();
    binary.show(empty, position, Move::Next);
    let displayed = binary.page.as_ref().unwrap();
    assert_eq!(displayed.offset, 0);
    assert_eq!(displayed.bytes, b"\x89PN");
    assert!(displayed.next_page.is_none());
}

#[gpui_kit::test]
fn hex_and_preview_stop_at_the_last_byte_without_fetching_a_terminator(cx: &mut TestAppContext) {
    let mock = Mock::new(|request| {
        if request.path.contains("&page=eof") {
            return (404, String::new());
        }
        let (status, body) = binary_response(request);
        if request.path.contains("&page=png-tail") {
            let mut page: serde_json::Value = serde_json::from_str(&body).unwrap();
            page["content"]["nextPage"] = json!("eof");
            (status, page.to_string())
        } else {
            (status, body)
        }
    });
    let (view, window) = mount(cx, &mock);
    click(window, ("crew-diff-bytes", 0usize));
    click(window, "crew-binary-next");
    assert_page(&view, window, DiffSide::New, 3);
    window.update(|window, _| assert!(window.try_find("crew-binary-next").is_none()));
    let _image = open_preview(&view, window);
    click(window, "crew-binary-return");
    click(window, "crew-binary-previous");
    assert_page(&view, window, DiffSide::New, 0);
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.contains("&page=eof"))
    );
}

fn click(window: &mut VisualTestContext, id: impl Into<ElementId>) {
    window.update(|window, cx| window.click(id, cx));
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
}

fn assert_page(
    view: &Entity<DiffView>,
    window: &mut VisualTestContext,
    side: DiffSide,
    offset: u64,
) {
    view.read_with(window, |view, _| {
        let binary = view.binary.as_ref().unwrap();
        let page = binary.page.as_ref().unwrap();
        assert_eq!((page.side, page.offset), (side, offset));
        assert!(binary.preview.is_none());
    });
}

fn open_preview(view: &Entity<DiffView>, window: &mut VisualTestContext) -> Arc<RenderImage> {
    click(window, "crew-binary-preview");
    let image = view.read_with(window, |view, _| {
        view.binary
            .as_ref()
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .image()
    });
    window.update(|window, _| assert!(window.has_image_atlas_entry(&image)));
    image
}

fn binary_response(request: &subspace_framework::client_test_support::Request) -> (u16, String) {
    let file = json!({"path": PATH, "status": "modified", "binary": true, "size": 70});
    if request.path.contains("/diff/file?") {
        assert!(request.path.contains("&at=B"));
        let side = if request.path.contains("&side=old") {
            "old"
        } else {
            "new"
        };
        let next = request.path.contains("&page=png-tail");
        let content = json!({"side": side, "encoding": "base64", "size": 70,
            "offset": if next { 3 } else { 0 }, "data": if next { PNG_TAIL } else { "iVBO" },
            "nextPage": (!next).then_some("png-tail")});
        return (
            200,
            json!({"fingerprint": "B", "file": file, "content": content}).to_string(),
        );
    }
    if request.path.contains("/diff?") {
        return (
            200,
            json!({"ship": "example-ship", "member": "example-crew", "mode": "branch",
            "fingerprint": "B", "computedAt": 1, "total": 1, "additions": 0, "deletions": 0,
            "files": [file]})
            .to_string(),
        );
    }
    (404, String::new())
}
