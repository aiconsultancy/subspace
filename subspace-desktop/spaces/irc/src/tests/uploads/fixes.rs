//! Review fixes: a `SendAllTogether` the server's limits refuse, Enter
//! again while queued, a missing or expired upload uploads afresh only on
//! an explicit send, and "Send as attachment…" waits for earlier files.
use super::super::completion::{composer, text};
use super::connect;
use super::tray::{
    BATCHING, answer, attach, network, posts, renew_and_resend, sent, server, status, submit,
};
use crate::uploads::outgoing::{self, ItemState, PREPARING, Phase};
use gpui_kit::TestAppContext;
use subspace_api_client::chat_uploads::{UploadError, references};
use subspace_client_irc::Event;

/// Sets the composer's text, as typing does.
fn typed(
    input: &gpui_kit::Entity<subspace_framework::dashboard::ChatInput>,
    text: &str,
    cx: &mut gpui_kit::VisualTestContext,
) {
    cx.update(|window, cx| input.update(cx, |input, cx| input.set_text(text, window, cx)));
}

#[gpui_kit::test]
fn a_draft_over_the_servers_batch_lines_sends_nothing_and_keeps_draft_and_tray(
    cx: &mut TestAppContext,
) {
    let (view, input, cx) = composer(cx);
    let (hub, room) = view.read_with(cx, |view, _| (view.hub.clone(), view.buffer.clone()));
    // The board knows the server batches; only the client knows how much.
    crate::tests::inject(
        &hub,
        vec![Event::Capabilities {
            network: room.network.clone(),
            enabled: vec!["batch".into(), "draft/multiline".into()],
        }],
        cx,
    );
    let mock = server();
    connect(&view, &mock, cx);
    let _folder = attach(&view, &["a.txt"], cx);
    let draft = "line\n".repeat(24) + "last";
    typed(&input, &draft, cx);
    submit(&view, &draft, cx);
    let lines = sent(&view, cx);
    assert_eq!(lines.len(), 2, "the text, then the reference: {lines:?}");
    let caps = BATCHING.replace("max-lines=100", "max-lines=1");
    assert!(
        answer(&view, &caps, cx).is_empty(),
        "nothing reached the server"
    );
    view.read_with(cx, |view, _| {
        assert!(view.tray.idle(), "ready to send again");
        assert_eq!(view.tray.items.len(), 1, "the tray stays");
    });
    assert_eq!(text(&input, cx), draft, "the draft stays");
    assert!(!status(&input, cx).is_empty(), "says why");
    // Echoes of the reference alone, or of nothing, never clear it.
    let key = references(&lines[1]).remove(0);
    crate::tests::inject(
        &hub,
        vec![Event::Line(crate::fixtures::message(
            &room, "subspace", &key, 6,
        ))],
        cx,
    );
    assert_eq!(text(&input, cx), draft);
    assert_eq!(view.read_with(cx, |view, _| view.tray.items.len()), 1);
    assert_eq!(posts(&mock, "a.txt"), 1);
}

#[gpui_kit::test]
fn enter_again_with_the_queued_text_sends_nothing_but_new_text_goes(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let mock = server();
    connect(&view, &mock, cx);
    let _folder = attach(&view, &["a.txt"], cx);
    typed(&input, "look", cx);
    submit(&view, "look", cx);
    let first = sent(&view, cx);
    assert_eq!(first.len(), 1);
    submit(&view, "look", cx);
    submit(&view, " look\n", cx);
    assert_eq!(sent(&view, cx), first, "no second copy before the echo");
    assert!(view.read_with(cx, |view, _| matches!(
        view.tray.phase,
        Phase::Queued { .. }
    )));
    submit(&view, "and more", cx);
    let after = sent(&view, cx);
    assert_eq!(after.len(), 2);
    assert_eq!(after[1], "and more");
    assert_eq!(posts(&mock, "a.txt"), 1);
}

#[gpui_kit::test]
fn a_missing_upload_found_by_check_uploads_afresh_on_the_next_enter(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let mock = server();
    connect(&view, &mock, cx);
    let _folder = attach(&view, &["gone.txt"], cx);
    input.update(cx, |_, cx| {
        cx.emit(subspace_framework::dashboard::ChatInputEvent::Submit(
            String::new(),
        ))
    });
    network(&view, false, cx);
    cx.run_until_parked();
    assert_eq!(posts(&mock, "gone.txt"), 1);
    renew_and_resend(&view, cx);
    let id = view.read_with(cx, |view, _| match &view.tray.items[0].state {
        ItemState::Ready(p) => p.draft.id().to_owned(),
        _ => panic!("the attachment stays ready"),
    });
    view.update(cx, |view, cx| outgoing::check(view, &id, cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| match &view.tray.items[0].state {
        ItemState::Ready(p) => {
            assert_eq!(p.error, Some(UploadError::Missing));
            assert!(!p.uncertain && p.uploaded.is_none(), "forgotten, file kept");
            assert_eq!(p.draft.bytes(), b"contents of gone.txt");
            assert_ne!(p.draft.id(), id, "a new upload ID");
        }
        _ => panic!("the attachment stays ready"),
    });
    assert_eq!(posts(&mock, "gone.txt"), 1, "never uploaded by itself");
    submit(&view, "", cx);
    assert_eq!(
        posts(&mock, "gone.txt"),
        2,
        "the explicit Enter uploads afresh"
    );
    assert_eq!(sent(&view, cx).len(), 1);
}

#[gpui_kit::test]
fn an_expired_upload_marks_its_file_and_a_retry_uploads_afresh(cx: &mut TestAppContext) {
    let (view, _, cx) = composer(cx);
    let mock = server();
    connect(&view, &mock, cx);
    let _folder = attach(&view, &["fine.txt", "old.txt"], cx);
    let first = view.read_with(cx, |view, _| match &view.tray.items[1].state {
        ItemState::Ready(p) => p.draft.id().to_owned(),
        _ => panic!("ready"),
    });
    submit(&view, "", cx);
    assert!(sent(&view, cx).is_empty(), "nothing sends");
    view.read_with(cx, |view, _| {
        let ItemState::Ready(fine) = &view.tray.items[0].state else {
            panic!("ready");
        };
        assert_eq!(fine.error, None, "only the expired file is marked");
        let ItemState::Ready(old) = &view.tray.items[1].state else {
            panic!("ready");
        };
        assert_eq!(old.error, Some(UploadError::Expired));
        assert!(old.uploaded.is_none());
    });
    assert_eq!(posts(&mock, "old.txt"), 1, "no automatic upload");
    submit(&view, "", cx);
    assert_eq!(posts(&mock, "old.txt"), 2);
    assert_eq!(posts(&mock, "fine.txt"), 1);
    let lines = sent(&view, cx);
    assert_eq!(lines.len(), 1);
    assert_eq!(references(&lines[0]).len(), 2);
    let renewed = view.read_with(cx, |view, _| match &view.tray.items[1].state {
        ItemState::Ready(p) => p.draft.id().to_owned(),
        _ => panic!("ready"),
    });
    assert_ne!(renewed, first, "a new upload ID");
}

#[gpui_kit::test]
fn send_as_attachment_waits_while_an_earlier_file_is_read(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("first.txt");
    std::fs::write(&path, "first").unwrap();
    view.update(cx, |view, cx| outgoing::paths(view, vec![path], cx));
    typed(&input, "my notes", cx);
    assert!(view.read_with(cx, |view, _| matches!(
        view.tray.items[0].state,
        ItemState::Preparing(_)
    )));
    cx.update(|window, cx| view.update(cx, |view, cx| outgoing::text(view, window, cx)));
    assert_eq!(status(&input, cx), PREPARING);
    assert_eq!(
        text(&input, cx),
        "my notes",
        "the text stays in the composer"
    );
    cx.run_until_parked();
    cx.update(|window, cx| view.update(cx, |view, cx| outgoing::text(view, window, cx)));
    view.read_with(cx, |view, _| {
        let names: Vec<&str> = view
            .tray
            .items
            .iter()
            .filter_map(|item| match &item.state {
                ItemState::Ready(p) => Some(p.draft.name()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["first.txt", "message.md"], "tray order kept");
    });
}
