mod draft;
mod fence;
mod standalone;
mod states;

use super::Content;
use crate::tests::{
    has, label,
    offline::{self, Offline, Switches, board_with_hub, settle_board, wait_for},
    press, render,
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px, test::TestWindowExt as _};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use subspace_framework::client_test_support::Mock;

const MARKDOWN: &str = "# Plan\n\n- One\n- [Guide](https://example.invalid/guide)\n\n![Remote](https://example.invalid/x.png)\n";
/// The Markdown file as an earlier read saw it, the same length.
const STALE: &str = "# Gone\n\n- One\n- [Guide](https://example.invalid/guide)\n\n![Remote](https://example.invalid/x.png)\n";
const PLAIN: &str = "Fixture notes\n  indented line\n";
/// Latin-1 bytes: not UTF-8.
const LATIN: &[u8] = b"caf\xe9";
const FILES: usize = 6;
const ISSUE: &str = "/api/v1/issues/SUB-1";
pub(super) const MARKDOWN_PATH: &str = "/api/v1/issues/attachments/md";

/// How the fixture answers attachment and issue reads.
#[derive(Default)]
pub(super) struct Reads {
    /// Refuse the plain-text read.
    pub(super) fail: AtomicBool,
    /// Hold the Markdown read until released.
    pub(super) hold_markdown: AtomicBool,
    /// Answer the Markdown read with `STALE`.
    pub(super) stale: AtomicBool,
    /// Hold the issue read, so a reload stays loading.
    pub(super) hold_issue: AtomicBool,
    /// Markdown reads answered so far.
    pub(super) answered: AtomicUsize,
}

fn file(id: &str, name: &str, mime: &str, size: usize) -> Value {
    json!({"mediaId":id,"kind":"file","fileName":name,"contentType":mime,"sizeBytes":size,
        "url":format!("/api/v1/issues/attachments/{id}"),"sha256":"fixture"})
}

fn attachments() -> Value {
    let huge = subspace_api_client::media::MAX_TEXT_BYTES + 1;
    json!({"items":[
        file("md", "Plan.md", "text/markdown", MARKDOWN.len()),
        file("txt", "Notes.txt", "text/plain", PLAIN.len()),
        file("pdf", "Spec.pdf", "application/pdf", 4),
        file("huge", "Huge.md", "text/markdown", huge),
        file("empty", "Empty.txt", "text/plain", 0),
        file("latin", "Latin.txt", "text/plain", LATIN.len()),
    ],"nextCursor":null})
}

fn markdown(reads: &Reads) -> &'static [u8] {
    wait_for(|| !reads.hold_markdown.load(Ordering::SeqCst));
    let body = if reads.stale.load(Ordering::SeqCst) {
        STALE
    } else {
        MARKDOWN
    };
    reads.answered.fetch_add(1, Ordering::SeqCst);
    body.as_bytes()
}

/// Attachment bytes by id.
fn bytes(path: &str, reads: &Reads) -> Option<(u16, Vec<(String, String)>, Vec<u8>)> {
    let id = path.strip_prefix("/api/v1/issues/attachments/")?;
    let (mime, body) = match id {
        "md" => ("text/markdown", markdown(reads)),
        "txt" if !reads.fail.load(Ordering::SeqCst) => ("text/plain", PLAIN.as_bytes()),
        "latin" => ("text/plain", LATIN),
        _ => return Some((500, Vec::new(), b"{}".to_vec())),
    };
    let headers = vec![
        ("Content-Type".into(), mime.into()),
        ("X-Content-Type-Options".into(), "nosniff".into()),
    ];
    Some((200, headers, body.to_vec()))
}

fn fixture(reads: Arc<Reads>) -> Offline {
    let switches = Arc::new(Switches::default());
    let state = switches.clone();
    let mock = Mock::with_bytes(move |r| {
        if let Some(answer) = bytes(&r.path, &reads) {
            return answer;
        }
        let body = if r.path.contains("/images") {
            json!({"items":[],"nextCursor":null})
        } else if r.path.contains("/attachments") {
            attachments()
        } else {
            let mut value = offline::api(&r.path, &state);
            if r.path == ISSUE {
                wait_for(|| !reads.hold_issue.load(Ordering::SeqCst));
                value["description"] = json!("Read [Notes](nett-media:txt) first.");
            }
            value
        };
        (200, Vec::new(), body.to_string().into_bytes())
    });
    Offline { mock, switches }
}

pub(super) fn opened(
    reads: Arc<Reads>,
    cx: &mut TestAppContext,
) -> (Offline, Entity<crate::Issues>, &mut VisualTestContext) {
    let fixture = fixture(reads);
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, crate::Issues::is_ready);
    board.update(cx, |board, cx| board.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| issue.files().items().len() == FILES)
    });
    render(cx);
    (fixture, board, cx)
}

pub(super) fn shows(
    board: &Entity<crate::Issues>,
    cx: &mut VisualTestContext,
    done: fn(&Content) -> bool,
) {
    settle_board(board, cx, |board| {
        board.media.text.as_ref().is_some_and(|v| done(&v.content))
    });
    render(cx);
}

pub(super) fn requested(fixture: &Offline, path: &str) -> usize {
    fixture
        .mock
        .requests()
        .iter()
        .filter(|r| r.path == path)
        .count()
}

#[gpui_kit::test]
fn markdown_opens_beside_the_page_without_remote_images_and_escape_closes_it_first(
    cx: &mut TestAppContext,
) {
    let (fixture, board, cx) = opened(Arc::default(), cx);
    assert!(!has(cx, "attachment-open:pdf"));
    cx.update(|window, cx| window.click("attachment-open:md", cx));
    shows(&board, cx, |content| {
        matches!(content, Content::Markdown(_))
    });
    assert!(has(cx, "attachment-viewer"));
    assert_eq!(
        label(cx, "attachment-viewer-size").as_deref(),
        Some("97 bytes")
    );
    assert_eq!(requested(&fixture, "/api/v1/issues/attachments/md"), 1);
    cx.update(|window, cx| {
        let panel = window.find("attachment-viewer").bounds();
        let markdown = window
            .find("rich-menu-Name(\"attachment-text:md:fixture\")")
            .bounds();
        assert!(markdown.size.height > panel.size.height / 2.);
        let viewer = board.read(cx).media.text.as_ref().unwrap();
        assert!(viewer.close.is_focused(window));
        let Content::Markdown(text) = &viewer.content else {
            panic!("the Markdown attachment is prepared");
        };
        assert!(text.contains("# Plan") && text.contains("Remote ↗"));
        assert!(!text.contains("!["));
    });
    cx.update(|_, cx| {
        let tokens = subspace_framework::dashboard::Tokens::current(cx);
        let width = super::panel_width(board.read(cx), &tokens);
        assert!(width >= subspace_framework::dashboard::panel::MIN_WIDTH);
    });
    press(cx, "escape");
    assert!(!has(cx, "attachment-viewer"));
    assert!(has(cx, "issues-detail"));
    cx.update(|_, cx| {
        let tokens = subspace_framework::dashboard::Tokens::current(cx);
        assert_eq!(super::panel_width(board.read(cx), &tokens), 0.);
    });
    press(cx, "escape");
    cx.update(|_, cx| assert!(board.read(cx).issue_view.snapshot.is_none()));
}

#[gpui_kit::test]
fn markdown_viewer_resizes_with_the_divider_and_resets_to_its_default(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = opened(Arc::default(), cx);
    cx.update(|window, cx| window.click("attachment-open:md", cx));
    shows(&board, cx, |content| {
        matches!(content, Content::Markdown(_))
    });
    let width = |cx: &mut VisualTestContext| {
        cx.update(|window, _| window.find("attachment-viewer").bounds().size.width)
    };
    let initial = width(cx);
    cx.update(|window, cx| {
        let from = window.find("attachment-viewer-resize").bounds().center();
        let mut to = from;
        to.x -= px(160.);
        window.drag(from, to, cx);
    });
    assert!(width(cx) > initial);

    cx.update(|window, cx| window.click("attachment-viewer-resize", cx));
    let after_drag = width(cx);
    press(cx, "right");
    assert_eq!(width(cx), after_drag - px(16.));
    cx.update(|window, cx| window.double_click("attachment-viewer-resize", cx));
    assert_eq!(width(cx), initial);
}

#[gpui_kit::test]
fn a_failed_read_offers_retry_and_plain_text_shows_after_it(cx: &mut TestAppContext) {
    let reads = Arc::new(Reads::default());
    reads.fail.store(true, Ordering::SeqCst);
    let (fixture, board, cx) = opened(reads.clone(), cx);
    // The description's reference opens the same panel as its Attachments row.
    assert!(has(cx, "attachment-open:txt"));
    cx.update(|window, cx| window.click("attachment-inline-open:txt", cx));
    shows(&board, cx, |content| matches!(content, Content::Failed));
    assert!(has(cx, "attachment-viewer-retry"));
    assert!(!has(cx, "attachment-viewer-text"));
    reads.fail.store(false, Ordering::SeqCst);
    cx.update(|window, cx| window.click("attachment-viewer-retry", cx));
    shows(&board, cx, |content| matches!(content, Content::Plain(_)));
    assert!(has(cx, "attachment-viewer-text"));
    assert_eq!(requested(&fixture, "/api/v1/issues/attachments/txt"), 2);
    cx.update(|window, cx| window.click("attachment-viewer-close", cx));
    render(cx);
    assert!(!has(cx, "attachment-viewer"));
}

#[gpui_kit::test]
fn oversized_text_is_refused_before_any_read(cx: &mut TestAppContext) {
    let (fixture, board, cx) = opened(Arc::default(), cx);
    cx.update(|window, cx| window.click("attachment-open:huge", cx));
    shows(&board, cx, |content| matches!(content, Content::TooLarge));
    assert!(has(cx, "attachment-viewer"));
    assert_eq!(requested(&fixture, "/api/v1/issues/attachments/huge"), 0);
}
