//! Second-round review fixes: a send the connection lost while
//! writing, or one made before the server welcomed us, keeps the draft and
//! tray and says why; nothing is sent again by itself.
use super::super::completion::{composer, text};
use super::connect;
use super::rehearse::{Rehearsed, rehearse};
use super::tray::{BATCHING, attach, sent, server, status, submit};
use gpui_kit::TestAppContext;

/// Sends "look" and one attachment, answered by a client connected as
/// `how`; checks that the draft, the tray and an explanation stay, and
/// returns what reached the server and the explanation.
fn kept_after(how: Rehearsed<'_>, cx: &mut TestAppContext) -> (Vec<String>, String) {
    let (view, input, cx) = composer(cx);
    let mock = server();
    connect(&view, &mock, cx);
    let _folder = attach(&view, &["a.txt"], cx);
    cx.update(|window, cx| input.update(cx, |input, cx| input.set_text("look", window, cx)));
    submit(&view, "look", cx);
    assert_eq!(sent(&view, cx).len(), 1, "one line went to the runtime");
    let wire = rehearse(&view, how, cx);
    view.read_with(cx, |view, _| {
        assert!(view.tray.idle(), "ready for an explicit send");
        assert_eq!(view.tray.items.len(), 1, "the tray stays");
    });
    assert_eq!(text(&input, cx), "look", "the draft stays");
    assert_eq!(sent(&view, cx).len(), 1, "nothing is sent again by itself");
    (wire, status(&input, cx))
}

#[gpui_kit::test]
fn a_send_lost_while_writing_keeps_the_draft_and_tray_and_says_check_first(
    cx: &mut TestAppContext,
) {
    let (wire, said) = kept_after(Rehearsed::LostWhileWriting(BATCHING), cx);
    assert_eq!(wire.len(), 1, "the line was being written");
    assert!(said.contains("some lines may have gone"), "{said}");
    assert!(said.contains("Check the channel"), "{said}");
}

#[gpui_kit::test]
fn a_send_before_the_welcome_sends_nothing_and_keeps_the_draft_and_tray(cx: &mut TestAppContext) {
    let (wire, said) = kept_after(Rehearsed::Registering(BATCHING), cx);
    assert!(wire.is_empty(), "no PRIVMSG before 001: {wire:?}");
    assert_eq!(said, "Not connected yet. Your message is kept.");
}
