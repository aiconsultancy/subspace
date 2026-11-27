use super::{file, has, offline, render, settle_board};
use crate::Issues;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use subspace_api_client::hub::Hub;
use subspace_api_client::uploads::ImageDeclaration;

fn fixture(server: Arc<AtomicU8>) -> offline::Offline {
    let mut media = file("draft", "Draft.txt", "text/plain", 5);
    media["sha256"] = json!("7743ce348d9284d677a185f33295b92266cc435a5b5f775029b300066d26693a");
    media["removal"] = json!("available");
    offline::offline_with(move |request| {
        let path = request.path.split('?').next().unwrap();
        let stage = server.load(Ordering::SeqCst);
        if path == "/api/v1/issues/attachments/draft" && request.method == "DELETE" {
            server.store(0, Ordering::SeqCst);
            return Some((200, "true".into()));
        }
        if path.ends_with("/comments") && request.method == "POST" {
            server.store(2, Ordering::SeqCst);
            return Some((201, json!({"id":"posted","issueId":"1","body":"Posted draft",
                "version":1,"actualAuthor":{"userId":"fixture-me","kind":"human","displayName":"Ada Fixture","isFormerMember":false},
                "createdAt":"2026-11-10T10:00:00Z","displayCreatedAt":"2026-11-10T10:00:00Z",
                "resolved":false,"replyCount":0,"hasReplies":false}).to_string()));
        }
        if !path.ends_with("/attachments") {
            return None;
        }
        if request.method == "POST" {
            server.store(1, Ordering::SeqCst);
            return Some((201, media.to_string()));
        }
        let mut listed = media.clone();
        if stage == 2 {
            listed["removal"] = json!("linked");
        }
        let items = if stage == 0 { vec![] } else { vec![listed] };
        Some((200, json!({"items":items,"nextCursor":null}).to_string()))
    })
}

fn attach(hub: &Hub, board: &Entity<Issues>, cx: &mut VisualTestContext) {
    let intake = hub
        .begin_comment_image_intake(hub.snapshot().generation(), "1", None)
        .unwrap();
    let declaration = ImageDeclaration::new("Draft.txt", "text/plain", b"draft").unwrap();
    intake
        .attach(declaration, Arc::from(b"draft".as_slice()))
        .unwrap();
    drop(intake);
    settle_board(board, cx, |_| {
        hub.comment_draft("1", None)
            .is_some_and(|d| d.attachments_ready())
    });
}

fn assert_protected(cx: &mut VisualTestContext) {
    render(cx);
    assert!(has(cx, "attachment-save:draft"));
    assert!(has(cx, "attachment-removal-notice:draft"));
    assert!(!has(cx, "attachment-remove:draft"));
}

#[gpui_kit::test]
fn comment_draft_holds_its_file_until_post_or_discard(cx: &mut TestAppContext) {
    let stage = Arc::new(AtomicU8::new(0));
    let fixture = fixture(stage.clone());
    let (board, cx) = offline::board_with_hub(&fixture, cx);
    board.update(cx, |board, cx| board.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .and_then(|open| open.detail())
            .is_some()
    });
    let hub = board.read_with(cx, |board, _| board.hub.clone().unwrap());
    attach(&hub, &board, cx);
    board.update(cx, |board, cx| board.reload_detail(cx));
    settle_board(&board, cx, |board| {
        board.issue_view.snapshot.as_ref().is_some_and(|open| {
            open.files()
                .items()
                .iter()
                .any(|media| media.media_id == "draft" && media.can_remove_attachment())
        })
    });
    assert_protected(cx);
    hub.remove_attachment("draft");
    assert!(!hub.snapshot().issue().unwrap().files().removing("draft"));
    assert!(!fixture.mock.requests().iter().any(|r| r.method == "DELETE"));
    hub.discard_comment("1", None);
    settle_board(&board, cx, |_| {
        stage.load(Ordering::SeqCst) == 0
            && hub
                .comment_draft("1", None)
                .is_none_or(|d| !d.has_attachments())
    });
    assert_eq!(
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        1
    );
    attach(&hub, &board, cx);
    post(&hub, &board, cx);
    assert_protected(cx);
}

fn post(hub: &Hub, board: &Entity<Issues>, cx: &mut VisualTestContext) {
    hub.edit_comment("1", None, "Posted draft").unwrap();
    hub.submit_comment("1", None, false).unwrap();
    settle_board(board, cx, |board| {
        board.issue_view.snapshot.as_ref().is_some_and(|open| {
            open.files().items().iter().any(|media| {
                media.media_id == "draft"
                    && media.attachment_removal_notice().is_some()
                    && !hub.comment_holds_attachment(media)
            })
        })
    });
}
