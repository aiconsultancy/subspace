//! The pointer contract a drawn session answers: a drag selects, a
//! finished drag offers its text for `copy-on-select`, a program that asks
//! for the mouse receives the click instead, shift selects over it anyway,
//! and the scrollbar drag moves the viewport (REQ-16, REQ-23).

use std::rc::Rc;

use gpui_kit::{
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Size,
    TestAppContext, WindowHandle, point, px, size,
};
use subspace_ghostty_config::GhosttyConfig;

use crate::command::SessionOptions;
use crate::pointer::{self, PointerTarget};
use crate::view::TerminalView;
use crate::view::clipboard;
use crate::view_tests::open_session;

/// The area every test in this module draws in.
fn area() -> Size<Pixels> {
    size(px(640.), px(400.))
}

/// A session that writes nothing of its own, so the screen stays as the
/// test drew it.
fn quiet_options() -> SessionOptions {
    SessionOptions {
        command: Some("/bin/cat".to_owned()),
        ..SessionOptions::default()
    }
}

/// Shift held down, which selects over a program's mouse reporting.
fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::default()
    }
}

/// A window showing a session with the given screen content.
fn session(bytes: &[u8], cx: &mut TestAppContext) -> WindowHandle<TerminalView> {
    let window = open_session(area(), &quiet_options(), &GhosttyConfig::default(), cx);
    let written = bytes.to_vec();
    window
        .update(cx, |view, window, _cx| {
            view.session_mut().terminal.vt_write(&written);
            view.state()
                .borrow_mut()
                .lay_out(gpui_kit::Bounds::new(point(px(0.), px(0.)), area()), window);
        })
        .expect("the session should take output");
    window
}

/// The pointer handle the element's listeners work on.
fn handle(view: &TerminalView) -> PointerTarget {
    PointerTarget {
        state: view.state(),
        pointer: Rc::clone(&view.pointer),
    }
}

/// A press of the left button at a position.
fn down(at: Point<Pixels>, modifiers: Modifiers) -> MouseDownEvent {
    MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: 1,
        first_mouse: false,
    }
}

/// A move with the left button held.
fn moved(at: Point<Pixels>, modifiers: Modifiers) -> MouseMoveEvent {
    MouseMoveEvent {
        position: at,
        pressed_button: Some(MouseButton::Left),
        modifiers,
    }
}

/// A release of the left button at a position.
fn up(at: Point<Pixels>, modifiers: Modifiers) -> MouseUpEvent {
    MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: 1,
    }
}

/// Drag from one position to another, as the element's three listeners do.
fn drag(
    window: &WindowHandle<TerminalView>,
    from: Point<Pixels>,
    to: Point<Pixels>,
    modifiers: Modifiers,
    cx: &mut TestAppContext,
) {
    window
        .update(cx, |view, _window, _cx| {
            let handle = handle(view);
            pointer::press(&handle, &down(from, modifiers));
            pointer::moved(&handle, &moved(to, modifiers));
            pointer::release(&handle, &up(to, modifiers));
        })
        .expect("the session should take the drag");
}

#[gpui_kit::test]
fn a_drag_selects_text_and_offers_it_for_copy_on_select(cx: &mut TestAppContext) {
    let window = session(b"hello world", cx);

    drag(
        &window,
        point(px(2.), px(2.)),
        point(px(42.), px(2.)),
        Modifiers::default(),
        cx,
    );
    let copied = window
        .update(cx, |view, _window, _cx| {
            view.pointer.borrow_mut().take_copied()
        })
        .expect("the view should report the selection");

    let copied = copied.expect("a finished drag offers its text");
    assert!(copied.starts_with("hello"), "{copied:?}");
}

#[gpui_kit::test]
fn a_program_that_asks_for_the_mouse_receives_the_click_and_starts_no_selection(
    cx: &mut TestAppContext,
) {
    let window = session(b"hello world\x1b[?1000h", cx);

    let (reported, selected) = window
        .update(cx, |view, _window, _cx| {
            let handle = handle(view);
            let reported =
                pointer::press(&handle, &down(point(px(10.), px(2.)), Modifiers::default()));
            let state = view.state();
            let selected = state
                .borrow()
                .session
                .terminal
                .selection()
                .expect("the emulator should answer")
                .is_some();
            (reported, selected)
        })
        .expect("the session should take the press");

    assert!(reported, "the click reaches the program");
    assert!(!selected, "a reporting program starts no selection");
}

#[gpui_kit::test]
fn shift_selects_over_a_program_that_asks_for_the_mouse(cx: &mut TestAppContext) {
    let window = session(b"hello world\x1b[?1000h", cx);

    drag(
        &window,
        point(px(2.), px(2.)),
        point(px(42.), px(2.)),
        shift(),
        cx,
    );
    let copied = window
        .update(cx, |view, _window, _cx| {
            view.pointer.borrow_mut().take_copied()
        })
        .expect("the view should report the selection");

    assert!(
        copied.is_some_and(|text| text.starts_with("hello")),
        "shift overrides mouse reporting (REQ-23)"
    );
}

#[gpui_kit::test]
fn a_double_click_selects_the_word_under_it(cx: &mut TestAppContext) {
    let window = session(b"hello world", cx);

    let copied = window
        .update(cx, |view, _window, _cx| {
            let handle = handle(view);
            let at = point(px(10.), px(2.));
            pointer::press(&handle, &down(at, Modifiers::default()));
            pointer::release(&handle, &up(at, Modifiers::default()));
            pointer::press(&handle, &down(at, Modifiers::default()));
            pointer::release(&handle, &up(at, Modifiers::default()));
            view.pointer.borrow_mut().take_copied()
        })
        .expect("the session should take the clicks");

    assert_eq!(
        copied.as_deref(),
        Some("hello"),
        "the second click selects the word (REQ-23)"
    );
}

#[gpui_kit::test]
fn the_session_offers_its_own_menu_only_while_the_program_leaves_the_pointer(
    cx: &mut TestAppContext,
) {
    let plain = session(b"hello", cx);
    let reporting = session(b"hello\x1b[?1000h", cx);

    let (free, taken) = (
        plain
            .update(cx, |view, _window, _cx| clipboard::offers_menu(view))
            .expect("the view should answer"),
        reporting
            .update(cx, |view, _window, _cx| clipboard::offers_menu(view))
            .expect("the view should answer"),
    );

    assert!(
        free,
        "a session with no mouse reporting offers Copy and Paste"
    );
    assert!(
        !taken,
        "a reporting program receives the right click (REQ-23)"
    );
}

#[gpui_kit::test]
fn a_scrollbar_drag_leaves_the_bottom_of_the_scrollback(cx: &mut TestAppContext) {
    let mut filled = Vec::new();
    for line in 1..=500 {
        filled.extend_from_slice(format!("{line}\r\n").as_bytes());
    }
    let window = session(&filled, cx);

    let (on_bar, followed) = window
        .update(cx, |view, _window, _cx| {
            let handle = handle(view);
            let on_bar = pointer::press(
                &handle,
                &down(point(px(638.), px(10.)), Modifiers::default()),
            );
            let followed = view
                .session()
                .terminal
                .viewport_active()
                .expect("the emulator should report its viewport");
            (on_bar, followed)
        })
        .expect("the session should take the press");

    assert!(on_bar, "the press lands on the scrollbar");
    assert!(!followed, "the drag moved the viewport into the scrollback");
}
