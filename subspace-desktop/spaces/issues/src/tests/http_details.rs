//! The real response status survives the hub, board and clipboard path.

use std::sync::atomic::Ordering;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::offline::{board_with_hub, offline, settle_board};
use super::render;
use crate::listing::Status;

#[gpui_kit::test]
fn a_real_500_copies_its_status_without_response_content(cx: &mut TestAppContext) {
    let fixture = offline();
    fixture.switches.fail_lists.store(true, Ordering::Relaxed);
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| {
        matches!(board.listing.status, Status::Failed(_))
    });
    render(cx);
    cx.update(|window, cx| window.click("issues-copy-details", cx));
    let text = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(text.contains("HTTP 500"), "{text}");
    assert!(text.contains("Issues list read"));
    for forbidden in ["fixture-org", "fixture-refresh", "Bearer", "{}"] {
        assert!(!text.contains(forbidden), "{text}");
    }
}
