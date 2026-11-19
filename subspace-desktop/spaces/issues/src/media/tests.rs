use crate::tests::{
    has, label,
    offline::{self, Offline, Switches, board_with_hub, settle_board},
    press, render,
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext, test::TestWindowExt as _};
use serde_json::{Value, json};
use std::sync::Arc;
use subspace_framework::client_test_support::Mock;

fn png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(2, 1)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
fn media(id: &str) -> Value {
    json!({"mediaId":id,"kind":"image","fileName":"Fixture.png","contentType":"image/png",
        "sizeBytes":png().len(),"url":"/api/v1/issues/media/a","sha256":"fixture"})
}
fn metadata(id: &str) -> Value {
    let mut value = media(id);
    value["id"] = json!(id);
    value["issueId"] = json!("1");
    value["createdAt"] = json!("2026-11-16T12:00:00Z");
    value["uploadedBy"] = json!({"userId":"fixture-me","kind":"human","displayName":"Ada Fixture","isFormerMember":false});
    value
}
fn fixture() -> Offline {
    let switches = Arc::new(Switches::default());
    let state = switches.clone();
    let mock = Mock::with_bytes(move |r| {
        if r.path.contains("/media/") {
            return (
                200,
                vec![
                    ("Content-Type".into(), "image/png".into()),
                    ("X-Content-Type-Options".into(), "nosniff".into()),
                ],
                png(),
            );
        }
        let body = if r.path.contains("/images") {
            if r.path.contains("cursor=next") {
                json!({"items":[metadata("c")],"nextCursor":null})
            } else {
                json!({"items":[metadata("a"),metadata("b")],"nextCursor":"next"})
            }
        } else if r.path.contains("/attachments") {
            json!({"items":[media("a")],"nextCursor":null})
        } else {
            let mut value = offline::api(&r.path, &state);
            if matches!(r.path.as_str(), "/api/v1/issues/SUB-1") {
                value["description"] = json!("![Fixture sketch](nett-media:a)");
            }
            value
        };
        (200, Vec::new(), body.to_string().into_bytes())
    });
    Offline { mock, switches }
}
fn opened(cx: &mut TestAppContext) -> (Offline, Entity<crate::Issues>, &mut VisualTestContext) {
    let fixture = fixture();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, crate::Issues::is_ready);
    board.update(cx, |board, cx| board.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|i| i.images().items().len() == 2 && !i.files().is_idle())
    });
    (fixture, board, cx)
}

#[gpui_kit::test]
fn inline_tiles_decode_private_bytes_and_viewer_pages_with_keyboard_and_returns_focus(
    cx: &mut TestAppContext,
) {
    let (fixture, board, cx) = opened(cx);
    render(cx);
    settle_board(&board, cx, |board| {
        super::view::descriptor(board, "a").is_some_and(|m| {
            matches!(
                board.media.cache.get(&super::key(&m)),
                Some(super::Preview::Ready(_))
            )
        })
    });
    assert!(has(cx, "image-inline:a"));
    assert!(
        fixture
            .mock
            .requests()
            .iter()
            .any(|r| r.path == "/api/v1/issues/media/a")
    );
    cx.update(|window, cx| window.click("image-inline-open:a", cx));
    render(cx);
    assert!(has(cx, "image-viewer"));
    assert_eq!(
        label(cx, "image-viewer-count").as_deref(),
        Some("1 of 2+ on this issue")
    );
    press(cx, "right");
    press(cx, "right");
    settle_board(&board, cx, |board| {
        board
            .media
            .viewer
            .as_ref()
            .is_some_and(|v| v.media.media_id == "c")
    });
    render(cx);
    assert_eq!(
        label(cx, "image-viewer-count").as_deref(),
        Some("3 of 3 on this issue")
    );
    press(cx, "escape");
    assert!(!has(cx, "image-viewer"));
    assert!(has(cx, "issues-detail"));
    cx.update(|window, cx| assert!(board.read(cx).media.focus["a"].is_focused(window)));
}

#[gpui_kit::test]
fn gallery_expand_overlays_thumbnail_and_restores_focus(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = opened(cx);
    render(cx);
    settle_board(&board, cx, |board| {
        super::view::descriptor(board, "a").is_some_and(|media| {
            matches!(
                board.media.cache.get(&super::key(&media)),
                Some(super::Preview::Ready(_))
            )
        })
    });
    render(cx);
    let action = "image-panel-open:a";
    assert_eq!(label(cx, action).as_deref(), Some("Open Fixture.png"));
    cx.update(|window, cx| {
        let tile = window.find("image-tile:a").bounds();
        let expand = window.find(action).bounds();
        assert!(expand.left() > tile.center().x && expand.right() <= tile.right());
        assert!(expand.top() >= tile.top() && expand.bottom() < tile.center().y);
        window.click(action, cx);
    });
    render(cx);
    assert!(has(cx, "image-viewer"));
    press(cx, "escape");
    assert!(!has(cx, "image-viewer"));
    cx.update(|window, cx| assert!(board.read(cx).media.focus["panel:a"].is_focused(window)));
    press(cx, "enter");
    assert!(has(cx, "image-viewer"));
}

#[gpui_kit::test]
fn oversized_previews_do_not_fetch_and_hidden_boards_release_decoded_memory(
    cx: &mut TestAppContext,
) {
    use subspace_framework::dashboard::Dashboard as _;
    let (fixture, board, cx) = opened(cx);
    let mut too_large: subspace_api_client::model::IssueMedia =
        serde_json::from_value(media("large")).unwrap();
    too_large.size_bytes = subspace_api_client::media::MAX_IMAGE_BYTES as i64 + 1;
    board.update(cx, |board, cx| board.ensure_image(too_large.clone(), cx));
    assert!(
        !fixture
            .mock
            .requests()
            .iter()
            .any(|r| r.path.contains("/media/large"))
    );
    cx.update(|window, cx| board.update(cx, |board, cx| board.deactivate(window, cx)));
    cx.update(|_, cx| assert_eq!(board.read(cx).media.cache.bytes(), 0));
}
