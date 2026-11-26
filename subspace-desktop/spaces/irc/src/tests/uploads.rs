mod delivery;
mod fixes;
mod intake;
mod multiline;
pub(super) mod received;
mod recovery;
mod rehearse;
mod saving;
mod tray;
// The composer keeps its text until every attachment uploaded and the lines went to IRC.
use super::completion::{composer, text};
use crate::uploads::outgoing;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

/// Signs the conversation's upload session in to `mock`'s API.
fn connect(
    view: &gpui_kit::Entity<crate::conversation::Conversation>,
    mock: &subspace_framework::client_test_support::Mock,
    cx: &mut gpui_kit::VisualTestContext,
) {
    let hub = view.read_with(cx, |view, _| view.hub.clone());
    let url = format!("{}/api", mock.base_url());
    let client = subspace_api_client::Client::loopback(&url, None).unwrap();
    hub.update(cx, |hub, _| hub.uploads.session.test_connect(client, "me"));
}

/// A rendered element id built at run time.
fn element(id: String) -> gpui_kit::SharedString {
    gpui_kit::SharedString::from(id)
}

#[gpui_kit::test]
fn dropped_documents_wait_as_thumbnails_and_remove_keeps_the_composer(cx: &mut TestAppContext) {
    let (conversation, input, cx) = composer(cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text("Keep my message", window, cx)
        })
    });
    let folder = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for extension in ["zip", "pdf", "docx", "xlsx"] {
        let path = folder.path().join(format!("captain.{extension}"));
        let bytes = if extension == "pdf" {
            &b"%PDF-1.7\n%\xff\xfe"[..]
        } else {
            &b"PK\x03\x04\xff\x00\xfe"[..]
        };
        std::fs::write(&path, bytes).unwrap();
        paths.push(path);
    }
    conversation.update(cx, |view, cx| outgoing::paths(view, paths, cx));
    cx.run_until_parked();
    let ids: Vec<usize> =
        conversation.read_with(cx, |c, _| c.tray.items.iter().map(|i| i.id).collect());
    assert_eq!(ids.len(), 4);
    cx.update(|window, cx| {
        window.render_frame(cx);
        for id in &ids {
            assert!(
                window
                    .try_find(element(format!("irc-attachment:{id}")))
                    .is_some()
            );
        }
        assert!(window.try_find("irc-confirm-attachment").is_none());
        assert!(window.try_find("irc-cancel-attachment").is_none());
    });
    for id in ids {
        cx.update(|window, cx| {
            conversation.update(cx, |c, cx| outgoing::remove(c, id, window, cx))
        });
    }
    cx.run_until_parked();
    assert_eq!(text(&input, cx), "Keep my message");
    assert!(conversation.read_with(cx, |view, _| view.tray.is_empty()));
}

#[gpui_kit::test]
fn send_as_attachment_moves_the_text_into_the_tray_and_remove_puts_it_back(
    cx: &mut TestAppContext,
) {
    let (conversation, input, cx) = composer(cx);
    let original = (1..=25)
        .map(|n| format!("Fixture line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.set_text(&original, window, cx));
        window.render_frame(cx);
        window.click("irc-send-attachment", cx);
    });
    cx.run_until_parked();
    assert_eq!(text(&input, cx), "");
    let id = conversation.read_with(cx, |c, _| {
        assert_eq!(c.tray.items.len(), 1);
        c.tray.items[0].id
    });
    let hub = conversation.read_with(cx, |c, _| c.hub.clone());
    assert!(hub.read_with(cx, |h, _| {
        h.sent()
            .iter()
            .all(|c| !matches!(c, subspace_client_irc::Command::Input { .. }))
    }));
    cx.update(|window, cx| conversation.update(cx, |c, cx| outgoing::remove(c, id, window, cx)));
    cx.run_until_parked();
    assert_eq!(text(&input, cx), original);
    assert!(conversation.read_with(cx, |c, _| c.tray.is_empty()));
}

#[gpui_kit::test]
fn a_viewer_opened_before_sign_in_offers_a_retry(cx: &mut TestAppContext) {
    let (conversation, _, cx) = composer(cx);
    let open = crate::uploads::viewer::claim(&conversation.downgrade());
    cx.update(|window, cx| open("UPL-THz3d1BhX6ieQk91Zzwrww", window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("chat-upload-viewer").is_some());
        assert!(window.try_find("chat-upload-retry").is_some());
        window.click("chat-upload-retry", cx);
    });
    cx.run_until_parked();
    assert!(conversation.read_with(cx, |c, _| c.upload_viewer.is_some()));
}
