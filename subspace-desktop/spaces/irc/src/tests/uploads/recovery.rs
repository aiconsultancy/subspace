//! Recovering a send: a queued tray, the tray-order total, a retry, a
//! dropped connection and a new session. Nothing uploads or sends twice.
use super::super::completion::composer;
use super::connect;
use super::tray::{attach, network, posts, renew_and_resend, send, sent, server, status, submit};
use crate::uploads::outgoing::{self, LIMIT_NOTICE, Phase};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

#[gpui_kit::test]
fn enter_while_queued_sends_plain_text_and_dismiss_clears_without_resending(
    cx: &mut TestAppContext,
) {
    let (view, input, cx) = composer(cx);
    let (lines, _api) = send(&view, &input, (&["a.txt"], "look"), cx);
    assert_eq!(lines.len(), 1);
    assert!(view.read_with(cx, |view, _| matches!(
        view.tray.phase,
        Phase::Queued { .. }
    )));
    submit(&view, "", cx);
    assert_eq!(sent(&view, cx), lines, "an empty Enter sends nothing again");
    submit(&view, "still here?", cx);
    let after = sent(&view, cx);
    assert_eq!(after.len(), 2, "{after:?}");
    assert_eq!(after[1], "still here?");
    assert!(subspace_api_client::chat_uploads::references(&after[1]).is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("irc-dismiss-attachments", cx);
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.tray.is_empty() && view.tray.idle()));
    assert_eq!(sent(&view, cx).len(), 2, "dismiss sends nothing");
}

#[gpui_kit::test]
fn the_total_is_checked_in_tray_order(cx: &mut TestAppContext) {
    let (view, _, cx) = composer(cx);
    let folder = tempfile::tempdir().unwrap();
    let sizes = [
        ("first.txt", 13_000_000),
        ("second.txt", 13_000_000),
        ("third.txt", 12_000_000),
    ];
    let paths = sizes
        .iter()
        .map(|(name, len)| {
            let path = folder.path().join(name);
            std::fs::write(&path, vec![b'a'; *len]).unwrap();
            path
        })
        .collect();
    view.update(cx, |view, cx| outgoing::paths(view, paths, cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let names: Vec<&str> = view
            .tray
            .items
            .iter()
            .filter_map(|item| match &item.state {
                outgoing::ItemState::Ready(p) => Some(p.draft.name()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            ["first.txt", "third.txt"],
            "25,000,000 bytes at most"
        );
        assert_eq!(view.tray.notice, Some(LIMIT_NOTICE));
    });
}

#[gpui_kit::test]
fn a_retry_uploads_only_what_failed(cx: &mut TestAppContext) {
    let (view, input, cx) = composer(cx);
    let (lines, (mock, _folder)) = send(&view, &input, (&["good.txt", "flaky.txt"], ""), cx);
    assert!(lines.is_empty());
    submit(&view, "", cx);
    let lines = sent(&view, cx);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        subspace_api_client::chat_uploads::references(&lines[0]).len(),
        2
    );
    assert_eq!(posts(&mock, "good.txt"), 1, "the finished upload is reused");
    assert_eq!(posts(&mock, "flaky.txt"), 2);
}

#[gpui_kit::test]
fn uploads_kept_over_a_reconnect_and_a_new_session_send_after_check_upload(
    cx: &mut TestAppContext,
) {
    let (view, input, cx) = composer(cx);
    let mock = server();
    connect(&view, &mock, cx);
    let _folder = attach(&view, &["kept.txt"], cx);
    // The connection drops while the file uploads: the test dispatcher runs
    // the upload only once the network is already down.
    input.update(cx, |_, cx| {
        cx.emit(subspace_framework::dashboard::ChatInputEvent::Submit(
            String::new(),
        ))
    });
    network(&view, false, cx);
    cx.run_until_parked();
    assert!(sent(&view, cx).is_empty());
    assert_eq!(
        status(&input, cx),
        "Uploaded. Reconnect, then press Enter to send."
    );
    assert_eq!(posts(&mock, "kept.txt"), 1);

    renew_and_resend(&view, cx);
    assert!(
        sent(&view, cx).is_empty(),
        "a new session never sends unchecked"
    );
    let id = view.read_with(cx, |view, _| match &view.tray.items[0].state {
        outgoing::ItemState::Ready(p) => {
            assert!(p.uncertain, "Check upload is offered");
            p.draft.id().to_owned()
        }
        _ => panic!("the attachment stays ready"),
    });
    view.update(cx, |view, cx| outgoing::check(view, &id, cx));
    cx.run_until_parked();
    submit(&view, "", cx);
    assert_eq!(sent(&view, cx).len(), 1);
    assert_eq!(posts(&mock, "kept.txt"), 1, "never uploaded twice");
}
