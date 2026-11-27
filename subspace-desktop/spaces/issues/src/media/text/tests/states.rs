//! The panel's notices, which offer no Retry, and Escape with a menu open.
use super::{file, opened, requested, shows};
use crate::media::text::Content;
use crate::tests::{has, label, press, render};
use gpui_kit::{TestAppContext, px, size, test::TestWindowExt as _};
use std::sync::Arc;
use subspace_api_client::media::MAX_TEXT_BYTES;
use subspace_api_client::model::IssueMedia;

fn media(size: usize) -> IssueMedia {
    serde_json::from_value(file("huge", "Huge.md", "text/markdown", size)).unwrap()
}

#[gpui_kit::test]
fn an_empty_file_says_so_without_a_read_or_retry(cx: &mut TestAppContext) {
    let (fixture, board, cx) = opened(Arc::default(), cx);
    cx.update(|window, cx| window.click("attachment-open:empty", cx));
    shows(&board, cx, |content| matches!(content, Content::Empty));
    assert!(has(cx, "attachment-viewer"));
    assert_eq!(
        label(cx, "attachment-viewer-size").as_deref(),
        Some("0 bytes")
    );
    assert!(!has(cx, "attachment-viewer-retry"));
    assert_eq!(requested(&fixture, "/api/v1/issues/attachments/empty"), 0);
}

#[gpui_kit::test]
fn bytes_that_are_not_utf8_show_a_notice_without_retry(cx: &mut TestAppContext) {
    let (fixture, board, cx) = opened(Arc::default(), cx);
    // The last card must be in the viewport before the simulated click.
    cx.simulate_resize(size(px(1440.), px(1200.)));
    render(cx);
    cx.update(|window, cx| window.click("attachment-open:latin", cx));
    shows(&board, cx, |content| matches!(content, Content::Unreadable));
    assert!(has(cx, "attachment-viewer"));
    assert!(!has(cx, "attachment-viewer-retry"));
    assert!(!has(cx, "attachment-viewer-text"));
    assert_eq!(requested(&fixture, "/api/v1/issues/attachments/latin"), 1);
}

#[gpui_kit::test]
fn escape_closes_an_open_menu_before_the_attachment(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = opened(Arc::default(), cx);
    cx.update(|window, cx| window.click("attachment-open:md", cx));
    shows(&board, cx, |content| {
        matches!(content, Content::Markdown(_))
    });
    cx.update(|window, cx| window.click("issue-more", cx));
    render(cx);
    cx.update(|_, cx| assert!(board.read(cx).menu.is_some()));
    press(cx, "escape");
    cx.update(|_, cx| assert!(board.read(cx).menu.is_none()));
    assert!(has(cx, "attachment-viewer"), "the first Escape closes More");
    press(cx, "escape");
    assert!(!has(cx, "attachment-viewer"), "the second closes the file");
    assert!(has(cx, "issues-detail"));
}

#[test]
fn the_size_limit_reads_in_exact_bytes_and_mib() {
    let over = MAX_TEXT_BYTES + 1;
    assert_eq!(
        crate::media::text::view::too_large(&media(over)),
        "Huge.md is 1,048,577 bytes. The viewer shows files up to 1 MiB (1,048,576 bytes)."
    );
    assert!(crate::media::text::view::too_large(&media(999)).starts_with("Huge.md is 999 bytes."));
    assert!(
        crate::media::text::view::too_large(&media(12_345_678))
            .starts_with("Huge.md is 12,345,678 bytes.")
    );
}
