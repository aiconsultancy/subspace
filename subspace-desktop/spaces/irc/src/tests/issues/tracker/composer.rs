//! The composer's keys and modes around a tracker command: Tab only
//! completes, and reply mode keeps a command and never sends it.

use gpui_kit::TestAppContext;

use super::{composer, labels, open, said, signed_in, text};

#[gpui_kit::test]
fn tab_on_a_whole_command_only_completes_and_writes_nothing(cx: &mut TestAppContext) {
    let mock = signed_in();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let input = composer(&conversation, cx);
    for typed in ["/ready NAV-9", " /assign NAV-9 to me"] {
        cx.update(|window, cx| input.update(cx, |input, cx| input.set_text("", window, cx)));
        cx.simulate_input(typed.trim_start());
        assert_eq!(
            labels(&conversation, cx).first().map(String::as_str),
            Some(typed.trim_start().split(' ').next().unwrap_or_default()),
            "the run row is highlighted first"
        );
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        assert!(
            text(&input, cx).starts_with(typed.trim_start()),
            "the draft stays: {}",
            text(&input, cx)
        );
    }
    assert!(mock.requests().is_empty(), "Tab never runs a command");
    conversation.read_with(cx, |conversation, _| {
        assert!(conversation.tracker.refreshed.is_empty())
    });
    hub.read_with(cx, |hub, _| {
        assert!(said(hub).is_empty(), "{:?}", said(hub))
    });
}
