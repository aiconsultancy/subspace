use super::{
    has,
    offline::{board_with_hub, offline, settle_board},
    render,
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
#[gpui_kit::test]
fn detail_exposes_comment_attachment_intake_without_issue_editing(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = open_first(&fixture, cx);
    render(cx);
    assert!(has(cx, "issue-comment-composer"));
    assert!(has(cx, "issue-comment-start"), "at rest it is one line");
    assert!(!has(cx, "attach-comment-image"));
    open_composer(&board, cx);
    assert!(has(cx, "issue-comment-composer"));
    assert!(!has(cx, "issue-comment-start"));
    assert!(!has(cx, "issue-edit"));
    assert!(has(cx, "attach-comment-image"));
}

/// Opens the root composer from its compact line at the end of the page.
fn open_composer(board: &Entity<crate::Issues>, cx: &mut VisualTestContext) {
    use gpui_kit::test::TestWindowExt as _;
    cx.update(|_, cx| board.read(cx).issue_view.scroll.scroll_to_bottom());
    render(cx);
    cx.update(|window, cx| window.click("issue-comment-start", cx));
    render(cx);
}

#[gpui_kit::test]
fn unclear_posts_require_a_discussion_check_before_an_explicit_post_again(cx: &mut TestAppContext) {
    use super::offline::offline_with;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let posts = Arc::new(AtomicUsize::new(0));
    let sent = posts.clone();
    let fixture = offline_with(move |request| {
        if request.method == "POST" && request.path.ends_with("/comments") {
            sent.fetch_add(1, Ordering::SeqCst);
            return Some((503, "{}".into()));
        }
        None
    });
    let (board, cx) = open_first(&fixture, cx);
    submit_text(&board, cx, None, "A retained draft");
    wait_unclear(&board, cx, false);
    render(cx);
    assert!(has(cx, "issue-comment-check"));
    assert!(!has(cx, "issue-comment-post-again"));
    click_composer_action(&board, cx, "issue-comment-check");
    wait_unclear(&board, cx, true);
    render(cx);
    assert_eq!(posts.load(Ordering::SeqCst), 1, "checking must never POST");
    assert!(has(cx, "issue-comment-post-again"));
    click_composer_action(&board, cx, "issue-comment-post-again");
    wait_unclear(&board, cx, false);
    assert_eq!(posts.load(Ordering::SeqCst), 2);
    cx.update(|_, cx| {
        assert_eq!(
            board
                .read(cx)
                .hub
                .as_ref()
                .unwrap()
                .comment_draft("1", None)
                .unwrap()
                .body(),
            "A retained draft"
        )
    });
}

pub(super) fn open_first<'a>(
    fixture: &super::offline::Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<crate::Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(fixture, cx);
    settle_board(&board, cx, |b| b.issue("issue:1").is_some());
    board.update(cx, |b, cx| b.open_row("issue:1", cx));
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .and_then(|i| i.detail())
            .is_some()
    });
    (board, cx)
}
fn wait_unclear(board: &Entity<crate::Issues>, cx: &mut VisualTestContext, refresh: bool) {
    settle_board(board, cx, |b| {
        b.hub
            .as_ref()
            .and_then(|h| h.comment_draft("1", None))
            .is_some_and(|d| {
                *d.state() == subspace_api_client::hub::CommentState::Unclear { refresh }
            })
    });
}

fn submit_text(
    board: &Entity<crate::Issues>,
    cx: &mut VisualTestContext,
    parent: Option<String>,
    text: &str,
) {
    use gpui_kit::test::TestWindowExt as _;
    if parent.is_none() {
        open_composer(board, cx);
    }
    cx.update(|window, cx| {
        board.update(cx, |board, cx| board.focus_comment(parent, window, cx));
        window.input(text, cx);
        window.press("secondary-enter", cx);
    });
}
fn posted_comment(parent: Option<&str>, body: &str) -> serde_json::Value {
    serde_json::json!({"id":"posted-fixture","issueId":"1","parentCommentId":parent,
        "body":body,"version":1,"actualAuthor":{"userId":"fixture-me","kind":"human","displayName":"Ada Fixture","isFormerMember":false},
        "createdAt":"2100-01-01T00:00:00Z","displayCreatedAt":"2100-01-01T00:00:00Z",
        "resolved":false,"replyCount":0,"hasReplies":false})
}
#[gpui_kit::test]
fn a_text_reply_posts_only_body_and_parent_then_clears_its_draft(cx: &mut TestAppContext) {
    let fixture = super::offline::offline_with(|request| {
        if request.method == "POST" && request.path.ends_with("/comments") {
            return Some((
                201,
                posted_comment(Some("comment-1"), "A text reply").to_string(),
            ));
        }
        if request.path.contains("/activity?") {
            let pages: serde_json::Value =
                serde_json::from_str(include_str!("../../fixtures/issue-timeline.json")).unwrap();
            return Some((200, pages["head"].to_string()));
        }
        None
    });
    let (board, cx) = open_first(&fixture, cx);
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .and_then(|i| i.timeline().page())
            .is_some()
    });
    submit_text(&board, cx, Some("comment-1".into()), "A text reply");
    settle_board(&board, cx, |b| {
        b.hub
            .as_ref()
            .and_then(|h| h.comment_draft("1", Some("comment-1")))
            .is_some_and(|draft| draft.notice().is_some())
    });
    let requests = fixture.mock.requests();
    let post = requests
        .iter()
        .find(|request| request.method == "POST")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&post.body).unwrap(),
        serde_json::json!({"body":"A text reply","parentCommentId":"comment-1"})
    );
    cx.update(|_, cx| {
        assert!(
            board
                .read(cx)
                .hub
                .as_ref()
                .unwrap()
                .comment_draft("1", Some("comment-1"))
                .unwrap()
                .body()
                .is_empty()
        )
    });
}
#[gpui_kit::test]
fn checking_a_matching_post_confirms_it_without_another_write(cx: &mut TestAppContext) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    // The server saves the comment but the reply is lost: it is in the
    // thread only after the POST, so the pre-POST snapshot cannot hold it.
    let posted = Arc::new(AtomicBool::new(false));
    let seen = posted.clone();
    let fixture = super::offline::offline_with(move |request| {
        if request.method == "POST" {
            seen.store(true, Ordering::SeqCst);
            return Some((503, "{}".into()));
        }
        if request.path.contains("/comments?") {
            let items = if seen.load(Ordering::SeqCst) {
                serde_json::json!([posted_comment(None, "A retained draft")])
            } else {
                serde_json::json!([])
            };
            return Some((
                200,
                serde_json::json!({"items":items,"nextCursor":null}).to_string(),
            ));
        }
        None
    });
    let (board, cx) = open_first(&fixture, cx);
    submit_text(&board, cx, None, "A retained draft");
    wait_unclear(&board, cx, false);
    render(cx);
    click_composer_action(&board, cx, "issue-comment-check");
    settle_board(&board, cx, |b| {
        b.hub
            .as_ref()
            .and_then(|h| h.comment_draft("1", None))
            .is_some_and(|d| d.body().is_empty() && d.confirmation() > 0)
    });
    render(cx);
    assert!(posted.load(Ordering::SeqCst));
    assert!(!has(cx, "issue-comment-post-again"));
    assert_eq!(
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
}

fn click_composer_action(
    board: &Entity<crate::Issues>,
    cx: &mut VisualTestContext,
    id: &'static str,
) {
    use gpui_kit::test::TestWindowExt as _;
    cx.update(|_, cx| board.read(cx).issue_view.scroll.scroll_to_bottom());
    render(cx);
    cx.update(|window, cx| window.click(id, cx));
}
