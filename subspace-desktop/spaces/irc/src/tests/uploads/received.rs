//! Received references: thumbnails replace resolved text, unavailable ones
//! keep the key, and the keyboard opens a thumbnail.
use super::super::completion::composer;
use super::element;
use crate::uploads::cache::Card;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use std::sync::Arc;
use subspace_api_client::Client;
use subspace_api_client::chat_uploads::{Upload, UploadDraft};
use subspace_client_irc::Event;

pub(in crate::tests) fn upload(name: &str) -> Upload {
    let draft = UploadDraft::new(name.into(), "text/markdown".into(), b"# hi".to_vec()).unwrap();
    serde_json::from_value(serde_json::json!({
        "id": draft.id(), "reference": draft.reference(), "fileName": name,
        "contentType": "text/markdown", "sizeBytes": 4, "sha256": "00",
        "uploadedByUserId": "someone", "createdAt": "2026-11-17T00:00:00Z",
        "expiresAt": "2099-01-01T00:00:00Z", "url": "/unused"
    }))
    .unwrap()
}

/// Signs the conversation in, caches `cards` and receives `text` from
/// someone else.
fn show(
    view: &gpui_kit::Entity<crate::conversation::Conversation>,
    cards: Vec<(String, Card)>,
    text: &str,
    cx: &mut gpui_kit::VisualTestContext,
) {
    let hub = view.read_with(cx, |view, _| view.hub.clone());
    let room = view.read_with(cx, |view, _| view.buffer.clone());
    cache(&hub, cards, cx);
    let line = crate::fixtures::message(&room, "galactica-captain", text, 5);
    crate::tests::inject(&hub, vec![Event::Line(line)], cx);
}

/// Signs `hub`'s upload session in and caches `cards`.
pub(in crate::tests) fn cache(
    hub: &gpui_kit::Entity<crate::hub::Hub>,
    cards: Vec<(String, Card)>,
    cx: &mut gpui_kit::VisualTestContext,
) {
    hub.update(cx, |hub, _| {
        let client = Client::loopback("http://127.0.0.1:9/api", None).unwrap();
        hub.uploads.session.test_connect(client, "me");
        for (key, card) in cards {
            hub.uploads.test_insert(key, card);
        }
    });
}

/// The key of the attachment the viewer shows, if it is open.
pub(in crate::tests) fn opened(
    view: &gpui_kit::Entity<crate::conversation::Conversation>,
    cx: &mut gpui_kit::VisualTestContext,
) -> Option<String> {
    view.read_with(cx, |view, _| {
        view.upload_viewer.as_ref().map(|v| v.key.clone())
    })
}

#[gpui_kit::test]
fn resolved_references_show_thumbnails_and_hide_their_text(cx: &mut TestAppContext) {
    let (view, _, cx) = composer(cx);
    let hub = view.read_with(cx, |view, _| view.hub.clone());
    let a = upload("plan.md");
    let b = upload("notes.md");
    let gone = upload("old.md");
    let text = format!(
        "see {} plan.md (4 bytes) {} notes.md (4 bytes) {} old.md (4 bytes)",
        a.reference, b.reference, gone.reference
    );
    let cards = vec![
        (a.reference.clone(), Card::Ready(Arc::new(a.clone()), None)),
        (b.reference.clone(), Card::Ready(Arc::new(b.clone()), None)),
        (gone.reference.clone(), Card::Expired),
    ];
    show(&view, cards, &text, cx);
    let shown = hub.read_with(cx, |hub, _| crate::uploads::shown_text(Some(hub), &text));
    assert_eq!(shown, format!("see {} old.md (4 bytes)", gone.reference));
    cx.update(|window, cx| {
        window.render_frame(cx);
        let id = |prefix: &str, upload: &Upload| element(format!("{prefix}:{}", upload.reference));
        for shown in [&a, &b, &gone] {
            assert!(window.try_find(id("chat-upload", shown)).is_some());
        }
        assert!(window.try_find(id("chat-upload-open", &a)).is_none());
        window.click(id("chat-upload", &a), cx);
    });
    cx.run_until_parked();
    assert_eq!(opened(&view, cx), Some(a.reference.clone()));
}

#[gpui_kit::test]
fn the_keyboard_reaches_a_thumbnail_and_enter_or_space_opens_it(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    cx.update(|_, cx| crate::conversation::bind_keys(cx));
    let a = upload("plan.md");
    let cards = vec![(a.reference.clone(), Card::Ready(Arc::new(a.clone()), None))];
    let text = format!("see https://example.org {} plan.md (4 bytes)", a.reference);
    show(&view, cards, &text, cx);
    cx.update(|window, cx| input.update(cx, |input, cx| input.focus(window, cx)));
    cx.simulate_keystrokes("shift-tab tab");
    cx.simulate_keystrokes("space");
    assert_eq!(opened(&view, cx), None, "Space opens attachments only");
    cx.simulate_keystrokes("tab");
    let on = view.read_with(cx, crate::conversation::focused_upload);
    assert_eq!(on, Some(a.reference.clone()));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(element(format!("chat-upload:{}", a.reference)))
                .is_some()
        );
    });
    cx.simulate_keystrokes("space");
    assert_eq!(opened(&view, cx), Some(a.reference.clone()));
    cx.simulate_keystrokes("escape");
    assert_eq!(opened(&view, cx), None);
    let back = view.read_with(cx, crate::conversation::focused_upload);
    assert_eq!(
        back,
        Some(a.reference.clone()),
        "closing returns to the thumbnail"
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(opened(&view, cx), Some(a.reference.clone()));
}

#[gpui_kit::test]
fn a_thumbnail_that_could_not_load_stays_clickable_and_a_missing_one_is_unavailable(
    cx: &mut TestAppContext,
) {
    let (view, _, cx) = composer(cx);
    let failed = upload("later.md");
    let missing = upload("gone.md");
    let cards = vec![
        (failed.reference.clone(), Card::Failed),
        (missing.reference.clone(), Card::Missing),
    ];
    let text = format!(
        "{} later.md (4 bytes) {} gone.md (4 bytes)",
        failed.reference, missing.reference
    );
    show(&view, cards, &text, cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(element(format!("chat-upload:{}", missing.reference)))
                .is_some()
        );
        window.click(element(format!("chat-upload:{}", failed.reference)), cx);
    });
    cx.run_until_parked();
    assert_eq!(opened(&view, cx), Some(failed.reference.clone()));
}
