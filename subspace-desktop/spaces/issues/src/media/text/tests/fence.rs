//! The read fence: a result for an earlier issue revision, a closed panel or
//! another attachment never shows. A real late answer is fenced by the hub
//! too, so each test also hands the board a late success directly, as a read
//! that finished just before the change would.
use super::{MARKDOWN_PATH, PLAIN, Reads, opened, requested, shows};
use crate::media::text::{Content, Fence};
use crate::tests::{
    has,
    offline::{Offline, settle_board, wait_for},
    render,
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext, test::TestWindowExt as _};
use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

/// Settle rounds a dropped answer gets to show, were it shown.
const IDLE_ROUNDS: usize = 20;

fn held() -> Arc<Reads> {
    let reads = Arc::new(Reads::default());
    reads.hold_markdown.store(true, Ordering::SeqCst);
    reads
}

/// The fence of a read of `media_id` started now.
fn fence(board: &Entity<crate::Issues>, cx: &mut VisualTestContext, media_id: &str) -> Fence {
    cx.update(|_, cx| {
        let media = &board.read(cx).media;
        Fence {
            scope: media.scope.unwrap(),
            revision: media.revision.unwrap(),
            media_id: media_id.into(),
        }
    })
}

/// A late Markdown success for `fence`.
fn late(board: &Entity<crate::Issues>, cx: &mut VisualTestContext, fence: &Fence) {
    let content = Content::Markdown(super::STALE.into());
    board.update(cx, |board, cx| board.finish_text(fence, content, cx));
    render(cx);
}

/// Lets a released answer reach the board, were it to be shown.
fn idle(board: &Entity<crate::Issues>, cx: &mut VisualTestContext) {
    for _ in 0..IDLE_ROUNDS {
        cx.run_until_parked();
        board.update(cx, |board, cx| board.poll_hub(cx));
        std::thread::sleep(Duration::from_millis(10));
    }
    render(cx);
}

fn content(board: &Entity<crate::Issues>, cx: &mut VisualTestContext) -> Option<Content> {
    cx.update(|_, cx| {
        board
            .read(cx)
            .media
            .text
            .as_ref()
            .map(|viewer| viewer.content.clone())
    })
}

fn loading(board: &Entity<crate::Issues>, cx: &mut VisualTestContext) -> bool {
    matches!(content(board, cx), Some(Content::Loading))
}

/// Opens Plan.md with its read held at the API.
fn opened_held(
    cx: &mut TestAppContext,
) -> (
    Arc<Reads>,
    Offline,
    Entity<crate::Issues>,
    &mut VisualTestContext,
) {
    let reads = held();
    let (fixture, board, cx) = opened(reads.clone(), cx);
    cx.update(|window, cx| window.click("attachment-open:md", cx));
    settle_board(&board, cx, |_| requested(&fixture, MARKDOWN_PATH) == 1);
    (reads, fixture, board, cx)
}

#[gpui_kit::test]
fn a_result_for_an_earlier_revision_is_dropped_and_the_panel_reads_again(cx: &mut TestAppContext) {
    let (reads, fixture, board, cx) = opened_held(cx);
    reads.stale.store(true, Ordering::SeqCst);
    let first = fence(&board, cx, "md");
    reads.hold_issue.store(true, Ordering::SeqCst);
    board.update(cx, |board, cx| board.reload_detail(cx));
    assert_ne!(fence(&board, cx, "md").revision, first.revision);
    reads.hold_markdown.store(false, Ordering::SeqCst);
    wait_for(|| reads.answered.load(Ordering::SeqCst) == 1);
    idle(&board, cx);
    assert!(
        loading(&board, cx),
        "the earlier revision's answer is dropped"
    );
    assert!(!has(cx, "attachment-viewer-retry"));
    late(&board, cx, &first);
    assert!(loading(&board, cx), "a late success is dropped too");
    reads.stale.store(false, Ordering::SeqCst);
    reads.hold_issue.store(false, Ordering::SeqCst);
    shows(&board, cx, |content| {
        matches!(content, Content::Markdown(_))
    });
    let Some(Content::Markdown(text)) = content(&board, cx) else {
        panic!("the panel reads again in the new revision");
    };
    assert!(text.contains("# Plan") && !text.contains("# Gone"));
    assert_eq!(requested(&fixture, MARKDOWN_PATH), 2);
}

#[gpui_kit::test]
fn closing_the_panel_mid_read_drops_its_late_result(cx: &mut TestAppContext) {
    let (reads, _fixture, board, cx) = opened_held(cx);
    let read = fence(&board, cx, "md");
    cx.update(|window, cx| window.click("attachment-viewer-close", cx));
    render(cx);
    assert!(!has(cx, "attachment-viewer"));
    late(&board, cx, &read);
    reads.hold_markdown.store(false, Ordering::SeqCst);
    wait_for(|| reads.answered.load(Ordering::SeqCst) == 1);
    idle(&board, cx);
    assert!(content(&board, cx).is_none(), "the panel stays closed");
    assert!(!has(cx, "attachment-viewer"));
    assert!(has(cx, "issues-detail"));
}

#[gpui_kit::test]
fn opening_another_attachment_mid_read_shows_only_the_new_one(cx: &mut TestAppContext) {
    let (reads, _fixture, board, cx) = opened_held(cx);
    let first = fence(&board, cx, "md");
    cx.update(|window, cx| window.click("attachment-open:txt", cx));
    shows(&board, cx, |content| matches!(content, Content::Plain(_)));
    late(&board, cx, &first);
    reads.hold_markdown.store(false, Ordering::SeqCst);
    wait_for(|| reads.answered.load(Ordering::SeqCst) == 1);
    idle(&board, cx);
    cx.update(|_, cx| {
        let viewer = board.read(cx).media.text.as_ref().unwrap();
        assert_eq!(viewer.media.media_id, "txt");
        let Content::Plain(text) = &viewer.content else {
            panic!("Notes.txt keeps its own text");
        };
        assert_eq!(text.as_ref(), PLAIN);
    });
    assert!(has(cx, "attachment-viewer-text"));
}
