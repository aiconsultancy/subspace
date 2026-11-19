//! The clipboard contract a drawn session answers: copy, paste, the
//! confirmation a paste that could run a command asks for, and the
//! sequences a program sends itself (REQ-22, REQ-24).

use gpui_kit::{ClipboardItem, Pixels, Size, TestAppContext, WindowHandle, px, size};
use subspace_ghostty_config::{ClipboardAccess, GhosttyConfig};

use crate::command::SessionOptions;
use crate::view::TerminalView;
use crate::view::clipboard;
use crate::view_tests::{UNCHANGED, drain_for, drain_until, open_session};

/// An OSC 52 write of `hi`.
const OSC52_WRITE_HI: &[u8] = b"\x1b]52;c;aGk=\x07";

/// The clipboard text a program reads back, as OSC 52 encodes it.
const SECRET: &str = "c2VjcmV0";

/// The area every test in this module draws in.
fn area() -> Size<Pixels> {
    size(px(640.), px(400.))
}

/// A session running `cat`, which echoes every byte the view sends it.
fn echo_options() -> SessionOptions {
    SessionOptions {
        command: Some("/bin/cat".to_owned()),
        ..SessionOptions::default()
    }
}

/// A window showing one session, styled from the given configuration.
fn session(config: &GhosttyConfig, cx: &mut TestAppContext) -> WindowHandle<TerminalView> {
    open_session(area(), &echo_options(), config, cx)
}

/// A window showing one session that has already received `bytes`.
fn session_showing(bytes: &[u8], cx: &mut TestAppContext) -> WindowHandle<TerminalView> {
    let window = session(&GhosttyConfig::default(), cx);
    let written = bytes.to_vec();
    window
        .update(cx, |view, _window, _cx| {
            view.session_mut().terminal.vt_write(&written);
        })
        .expect("the session should take output");
    window
}

/// Select the whole screen, as a triple click would.
fn select_all(window: &WindowHandle<TerminalView>, cx: &mut TestAppContext) {
    window
        .update(cx, |view, _window, _cx| {
            let state = view.state();
            let state = state.borrow();
            let all = state
                .session
                .terminal
                .select_all()
                .expect("the emulator should answer")
                .expect("the screen holds text");
            state
                .session
                .terminal
                .set_selection(Some(&all))
                .expect("the selection should install");
        })
        .expect("the session should take the selection");
}

#[gpui_kit::test]
fn copy_puts_the_selection_on_the_clipboard_and_does_nothing_without_one(cx: &mut TestAppContext) {
    let window = session_showing(b"copied text", cx);
    cx.write_to_clipboard(ClipboardItem::new_string("untouched".to_owned()));

    window
        .update(cx, |view, _window, cx| clipboard::copy(view, cx))
        .expect("the view should copy");
    let without = cx.read_from_clipboard().and_then(|item| item.text());
    select_all(&window, cx);
    window
        .update(cx, |view, _window, cx| clipboard::copy(view, cx))
        .expect("the view should copy");
    let with = cx.read_from_clipboard().and_then(|item| item.text());

    assert_eq!(
        without.as_deref(),
        Some("untouched"),
        "copy with no selection does nothing (REQ-22)"
    );
    assert!(
        with.is_some_and(|text| text.contains("copied text")),
        "the selection reaches the clipboard"
    );
}

#[gpui_kit::test]
fn a_paste_that_could_run_a_command_is_confirmed_before_it_is_sent(cx: &mut TestAppContext) {
    let window = session(&GhosttyConfig::default(), cx);
    cx.write_to_clipboard(ClipboardItem::new_string("one\ntwo".to_owned()));

    window
        .update(cx, |view, _window, cx| clipboard::paste(view, cx))
        .expect("the view should take the paste");
    let asked = window
        .update(cx, |view, _window, _cx| view.pending_paste.clone())
        .expect("the view should report the paste");
    let before = drain_for(&window, cx, UNCHANGED, |screen| screen.contains("two"));
    window
        .update(cx, |view, _window, cx| clipboard::send_pending(view, cx))
        .expect("the view should send the paste");
    let after = drain_until(&window, cx, |screen| screen.contains("two"));

    assert_eq!(asked.as_deref(), Some("one\ntwo"), "the paste waits");
    assert!(!before, "nothing is sent before the confirmation (REQ-22)");
    assert!(after, "the confirmed paste reaches the program");
}

#[gpui_kit::test]
fn a_paste_the_program_brackets_is_sent_at_once(cx: &mut TestAppContext) {
    let window = session_showing(b"\x1b[?2004h", cx);
    cx.write_to_clipboard(ClipboardItem::new_string("one\ntwo".to_owned()));

    window
        .update(cx, |view, _window, cx| clipboard::paste(view, cx))
        .expect("the view should take the paste");
    let (asked, failure) = window
        .update(cx, |view, _window, _cx| {
            (view.pending_paste.clone(), view.failure())
        })
        .expect("the view should report the paste");

    assert_eq!(asked, None, "a bracketed paste is safe (REQ-22)");
    assert_eq!(failure, None, "the paste should reach the program");
}

#[gpui_kit::test]
fn a_cancelled_paste_is_dropped(cx: &mut TestAppContext) {
    let window = session(&GhosttyConfig::default(), cx);
    cx.write_to_clipboard(ClipboardItem::new_string("one\ntwo".to_owned()));

    let dropped = window
        .update(cx, |view, _window, cx| {
            clipboard::paste(view, cx);
            clipboard::cancel_pending(view, cx);
            view.pending_paste.clone()
        })
        .expect("the view should drop the paste");

    assert_eq!(dropped, None);
}

#[gpui_kit::test]
fn a_programs_clipboard_write_reaches_the_system_clipboard(cx: &mut TestAppContext) {
    let window = session(&GhosttyConfig::default(), cx);
    cx.write_to_clipboard(ClipboardItem::new_string("before".to_owned()));

    window
        .update(cx, |view, _window, _cx| {
            view.session_mut().terminal.vt_write(OSC52_WRITE_HI);
        })
        .expect("the session should take the sequence");
    window
        .update(cx, |view, _window, cx| clipboard::exchange(view, cx))
        .expect("the view should exchange the clipboard");
    let written = cx.read_from_clipboard().and_then(|item| item.text());

    assert_eq!(written.as_deref(), Some("hi"), "OSC 52 writes (REQ-24)");
}

#[gpui_kit::test]
fn a_programs_clipboard_read_is_refused_unless_the_configuration_allows_it(
    cx: &mut TestAppContext,
) {
    let allowed = GhosttyConfig {
        clipboard_read: ClipboardAccess::Allow,
        ..GhosttyConfig::default()
    };
    let refused = session(&GhosttyConfig::default(), cx);
    let answered = session(&allowed, cx);
    cx.write_to_clipboard(ClipboardItem::new_string("secret".to_owned()));

    let refused_reply = read_reply(&refused, cx);
    let answered_reply = read_reply(&answered, cx);

    assert!(
        !refused_reply.contains(SECRET) && !refused_reply.contains("secret"),
        "no reply and no content with no configuration: {refused_reply:?}"
    );
    assert!(
        answered_reply.contains(SECRET),
        "clipboard-read = allow answers the program: {answered_reply:?}"
    );
}

/// What a program reads back after asking for the clipboard with OSC 52.
fn read_reply(window: &WindowHandle<TerminalView>, cx: &mut TestAppContext) -> String {
    window
        .update(cx, |view, _window, cx| {
            clipboard::exchange(view, cx);
            view.session_mut().terminal.vt_write(b"\x1b]52;c;?\x07");
            let state = view.state();
            let replies = state.borrow().clipboard.borrow_mut().take_replies();
            String::from_utf8_lossy(&replies).into_owned()
        })
        .expect("the view should report the reply")
}
