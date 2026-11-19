//! The input contract a real program answers: the keys and the committed
//! text reach it, a `keybind` line replaces a key's own encoding, and marked
//! text never leaves the view (REQ-18, REQ-31a).

use gpui_kit::{
    Bounds, EntityInputHandler as _, KeyDownEvent, Keystroke, Modifiers, Pixels, Size,
    TestAppContext, WindowHandle, px, size,
};
use subspace_ghostty_config::GhosttyConfig;

use crate::command::SessionOptions;
use crate::typing;
use crate::view::TerminalView;
use crate::view_tests::{UNCHANGED, drain_for, drain_until, open_session};

/// The area every test in this module draws in.
fn area() -> Size<Pixels> {
    size(px(640.), px(400.))
}

/// Numbered lines, which fill the scrollback (REQ-16).
fn numbered_lines(count: u32) -> Vec<u8> {
    (1..=count).fold(Vec::new(), |mut output, line| {
        output.extend_from_slice(format!("{line}\r\n").as_bytes());
        output
    })
}

/// A session running `cat`, which echoes every byte the view sends it.
fn echo_options() -> SessionOptions {
    SessionOptions {
        command: Some("/bin/cat".to_owned()),
        ..SessionOptions::default()
    }
}

/// A keystroke of one key with the given modifiers, as GPUI reports it.
fn keystroke(key: &str, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.to_owned(),
        key_char: printable(key).then(|| key.to_owned()),
    }
}

/// Whether GPUI would report a key name as the text it types.
fn printable(key: &str) -> bool {
    key.chars().count() == 1
}

/// Press one key through the view's own key listener.
fn press(window: &WindowHandle<TerminalView>, key: &str, cx: &mut TestAppContext) {
    press_with(window, key, Modifiers::default(), cx);
}

/// Press one key with modifiers through the view's own key listener.
fn press_with(
    window: &WindowHandle<TerminalView>,
    key: &str,
    modifiers: Modifiers,
    cx: &mut TestAppContext,
) {
    let event = KeyDownEvent {
        keystroke: keystroke(key, modifiers),
        is_held: false,
        prefer_character_input: false,
    };
    window
        .update(cx, |view, window, cx| view.key_down(&event, window, cx))
        .expect("the view should take the key");
}

#[gpui_kit::test]
fn typing_reaches_the_program_and_a_keybind_line_replaces_a_key(cx: &mut TestAppContext) {
    let config = GhosttyConfig {
        keybind: vec!["ctrl+b=text:replaced\\r".to_owned()],
        ..GhosttyConfig::default()
    };
    let window = open_session(area(), &echo_options(), &config, cx);

    press(&window, "h", cx);
    press(&window, "i", cx);
    press(&window, "enter", cx);
    let typed = drain_until(&window, cx, |screen| screen.contains("hi"));
    press_with(
        &window,
        "b",
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
        cx,
    );
    let bound = drain_until(&window, cx, |screen| screen.contains("replaced"));

    assert!(typed, "the keys should reach the program");
    assert!(
        bound,
        "the keybind line should send its own bytes (REQ-31a)"
    );
}

#[gpui_kit::test]
fn shift_and_page_scroll_the_session_instead_of_reaching_the_program(cx: &mut TestAppContext) {
    let window = open_session(area(), &echo_options(), &GhosttyConfig::default(), cx);
    window
        .update(cx, |view, _window, _cx| {
            view.session_mut().terminal.vt_write(&numbered_lines(500));
        })
        .expect("the session should take output");
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };

    press_with(&window, "pageup", shift, cx);
    let held = window
        .update(cx, |view, _window, _cx| {
            view.session()
                .terminal
                .viewport_active()
                .expect("the emulator should report its viewport")
        })
        .expect("the view should report its viewport");

    assert!(!held, "Shift+Page Up leaves the bottom of the scrollback");
}

#[gpui_kit::test]
fn a_composition_stays_in_the_view_until_the_input_method_commits_it(cx: &mut TestAppContext) {
    let window = open_session(area(), &echo_options(), &GhosttyConfig::default(), cx);

    let (marked, cursor) = window
        .update(cx, |view, window, cx| {
            view.replace_and_mark_text_in_range(None, "に", None, window, cx);
            (
                view.marked_text_range(window, cx),
                view.bounds_for_range(0..1, Bounds::default(), window, cx),
            )
        })
        .expect("the view should mark the composition");
    let composing = drain_for(&window, cx, UNCHANGED, |screen| screen.contains('に'));
    window
        .update(cx, |view, window, cx| {
            view.replace_text_in_range(None, "日", window, cx);
        })
        .expect("the view should commit the composition");
    let committed = drain_until(&window, cx, |screen| screen.contains('日'));
    let still_marked = window
        .update(cx, |view, window, cx| view.marked_text_range(window, cx))
        .expect("the view should report its composition");

    assert_eq!(marked, Some(0..1), "the marked text is the composition");
    assert!(cursor.is_some(), "an input method needs the cursor cell");
    assert!(
        !composing,
        "the program never sees uncommitted marked text (REQ-18)"
    );
    assert!(committed, "the committed text reaches the program");
    assert_eq!(still_marked, None, "a commit ends the composition");
}

#[gpui_kit::test]
fn a_composition_is_placed_at_the_cursor_and_clipped_to_its_row(cx: &mut TestAppContext) {
    let window = open_session(area(), &echo_options(), &GhosttyConfig::default(), cx);
    press(&window, "a", cx);
    assert!(
        drain_until(&window, cx, |screen| screen.contains('a')),
        "the program should echo the key"
    );

    let (at, columns) = window
        .update(cx, |view, _window, _cx| {
            let state = view.state();
            let state = state.borrow();
            (typing::caret(&state), state.session.size().columns)
        })
        .expect("the view should report its cursor");

    assert!(
        at.offset.x > px(0.),
        "the cursor has moved past the padding"
    );
    assert!(at.offset.y >= px(0.), "the cursor sits inside the bounds");
    assert!(
        at.rect.origin.x >= at.offset.x,
        "the window rectangle is the offset from the session's own origin"
    );
    let left = at.columns_left;
    assert!(
        left < usize::from(columns) && left > 0,
        "one column is used, the rest are left: {left} of {columns}"
    );
}
