//! The view drawn in a real test window: the resize contract REQ-17 states,
//! which only the element can answer, and the input contract of REQ-18 and
//! REQ-31a, which only a real program can answer. These check the contract
//! between the element, the view and the session, not where anything lands
//! on screen.
//!
//! They drive `/bin/sh`, `stty` and `cat` as their program. A ConPTY pseudo
//! console repaints the screen itself, so on Windows the session tests cover
//! the pseudo console instead.
#![cfg(unix)]

use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Focusable as _, Pixels, Size, TestAppContext, VisualTestContext,
    WindowHandle, point, px, size,
};
use libghostty_vt::style::RgbColor;
use subspace_ghostty_config::{GhosttyConfig, Rgb, ThemeValues};

use crate::command::SessionOptions;
use crate::grid::SessionSize;
use crate::metrics::{fit_grid, session_size};
use crate::pointer::Pointer;
use crate::snapshot::FrameReader;
use crate::state::TerminalState;
use crate::style::TerminalStyle;
use crate::view::TerminalView;

mod clipboard;
mod input;
mod pointer;

/// A session that reads from its pseudo-terminal and writes nothing, so the
/// screen stays as the test drew it.
fn options() -> SessionOptions {
    SessionOptions {
        command: Some("/bin/sh".to_owned()),
        ..SessionOptions::default()
    }
}

/// A background colour no token produces, so a test can tell it apart.
const MARKER: Rgb = Rgb {
    r: 0x12,
    g: 0x34,
    b: 0x56,
};

/// How long a test waits for a real program.
const WAIT: Duration = Duration::from_secs(10);

/// How often a test drains while waiting.
const POLL: Duration = Duration::from_millis(10);

/// How long a test waits before it accepts that the program received
/// nothing. A session echoes a byte it did receive in well under this.
pub(super) const UNCHANGED: Duration = Duration::from_millis(300);

/// Numbered lines, as `seq` produces them (REQ-16).
fn lines(count: u32) -> Vec<u8> {
    (1..=count).fold(Vec::new(), |mut output, line| {
        output.extend_from_slice(format!("{line}\r\n").as_bytes());
        output
    })
}

/// An area at the window's own origin, which is what the element lays the
/// session out in.
fn bounds(area: Size<Pixels>) -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), area)
}

/// Whether the view follows new output, which is true only at the bottom.
fn follows_output(view: &TerminalView) -> bool {
    view.session()
        .terminal
        .viewport_active()
        .expect("the emulator should report its viewport")
}

/// A window of the given size showing one session, drawn once.
fn open(area: Size<Pixels>, cx: &mut TestAppContext) -> WindowHandle<TerminalView> {
    open_session(area, &options(), &GhosttyConfig::default(), cx)
}

/// A window showing one session of the given program, styled from the given
/// configuration, drawn once.
pub(super) fn open_session(
    area: Size<Pixels>,
    options: &SessionOptions,
    config: &GhosttyConfig,
    cx: &mut TestAppContext,
) -> WindowHandle<TerminalView> {
    let style = cx.update(|cx| {
        gpui_kit::init(cx);
        let (style, warnings) = TerminalStyle::resolve(config, &ThemeValues::default(), cx);
        assert!(warnings.is_empty(), "{warnings:?}");
        style
    });
    let session = match crate::Session::start(options, SessionSize::default()) {
        Ok(session) => session,
        Err(error) => panic!("the session should start: {error}"),
    };
    let state = TerminalState::new(session, FrameReader::new().expect("a frame reader"), style)
        .expect("the render state should start");
    let pointer = Pointer::new().expect("the pointer should start");
    let window = cx.open_window(area, move |_window, cx| {
        TerminalView::new(state, pointer, cx)
    });
    cx.update_window(window.into(), |_root, window, cx| window.render_frame(cx))
        .expect("the window should draw");
    window
}

/// The text of the last frame the view built, one line per row.
pub(super) fn screen(view: &TerminalView) -> String {
    let state = view.state();
    let state = state.borrow();
    state
        .painted
        .frame
        .rows
        .iter()
        .map(|row| {
            row.runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Apply the session's output and rebuild the frame until the screen
/// satisfies the condition, or the wait runs out.
pub(super) fn drain_until(
    window: &WindowHandle<TerminalView>,
    cx: &mut TestAppContext,
    ready: impl Fn(&str) -> bool,
) -> bool {
    drain_for(window, cx, WAIT, ready)
}

/// Apply the session's output and rebuild the frame for at most `wait`,
/// reporting whether the screen ever satisfied the condition.
pub(super) fn drain_for(
    window: &WindowHandle<TerminalView>,
    cx: &mut TestAppContext,
    wait: Duration,
    ready: impl Fn(&str) -> bool,
) -> bool {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        let shown = window
            .update(cx, |view, _window, _cx| {
                view.state().borrow_mut().drain();
                screen(view)
            })
            .expect("the view should drain");
        cx.update_window((*window).into(), |_root, window, cx| {
            window.render_frame(cx)
        })
        .expect("the window should draw");
        if ready(&shown) {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

#[gpui_kit::test]
fn drawing_fits_the_grid_to_the_window_and_tells_the_session(cx: &mut TestAppContext) {
    let window = open(size(px(640.), px(400.)), cx);

    let (reported, expected) = window
        .update(cx, |view, window, _cx| {
            let state = view.state();
            let state = state.borrow();
            let cell = state.measure(window);
            let fitted = fit_grid(640.0, 400.0, state.style.padding, cell).expect("a grid fits");
            (state.session.size(), session_size(fitted.0, fitted.1, cell))
        })
        .expect("the view should report its grid");

    assert_eq!(reported, expected);
    assert_ne!(
        reported,
        SessionSize::default(),
        "the drawn grid replaces the size the session started on"
    );
}

#[gpui_kit::test]
fn a_window_too_small_for_one_cell_leaves_the_session_alone(cx: &mut TestAppContext) {
    let window = open(size(px(6.), px(6.)), cx);

    let reported = window
        .update(cx, |view, _window, _cx| view.session().size())
        .expect("the view should report its grid");

    assert_eq!(reported, SessionSize::default());
}

#[gpui_kit::test]
fn scrolling_back_holds_the_viewport_and_the_bottom_follows_again(cx: &mut TestAppContext) {
    let window = open(size(px(640.), px(400.)), cx);
    window
        .update(cx, |view, _window, _cx| {
            view.session_mut().terminal.vt_write(&lines(500));
        })
        .expect("the session should take output");

    let (at_bottom, held, output_held, followed) = window
        .update(cx, |view, _window, cx| {
            let at_bottom = follows_output(view);
            view.scroll_pages(-1.0, cx);
            let held = follows_output(view);
            view.session_mut().terminal.vt_write(&lines(10));
            let output_held = follows_output(view);
            view.scroll_to_bottom(cx);
            (at_bottom, held, output_held, follows_output(view))
        })
        .expect("the view should scroll");

    assert!(at_bottom, "a fresh session follows its output");
    assert!(!held, "a page back holds the position");
    assert!(!output_held, "new output does not pull the view back down");
    assert!(followed, "returning to the bottom resumes following");
}

#[gpui_kit::test]
fn the_wheel_and_the_scrollbar_reach_the_same_scrollback(cx: &mut TestAppContext) {
    let window = open(size(px(640.), px(400.)), cx);
    window
        .update(cx, |view, _window, _cx| {
            view.session_mut().terminal.vt_write(&lines(500));
        })
        .expect("the session should take output");

    let (wheeled, dragged) = window
        .update(cx, |view, _window, cx| {
            view.state().borrow_mut().scroll_wheel(3.0);
            let wheeled = !follows_output(view);
            view.scroll_to_fraction(0.0, cx);
            let dragged = !follows_output(view);
            (wheeled, dragged)
        })
        .expect("the view should scroll");

    assert!(wheeled, "a wheel movement leaves the bottom");
    assert!(dragged, "the top of the track is in the scrollback");
}

#[gpui_kit::test]
fn the_title_the_program_sets_reaches_the_tab(cx: &mut TestAppContext) {
    let window = open(size(px(640.), px(400.)), cx);

    let (untitled, titled) = window
        .update(cx, |view, _window, _cx| {
            let untitled = view.title();
            view.session_mut()
                .terminal
                .vt_write(b"\x1b]0;vim README\x07");
            view.state().borrow_mut().read_title();
            (untitled, view.title())
        })
        .expect("the session should take output");

    assert_eq!(untitled, None, "a tab shows the program name until then");
    assert_eq!(titled.as_deref(), Some("vim README"));
}

#[gpui_kit::test]
fn a_restyle_recolours_the_session_without_a_restart(cx: &mut TestAppContext) {
    let window = open(size(px(640.), px(400.)), cx);
    let restyled = cx.update(|cx| {
        let config = GhosttyConfig {
            background: Some(MARKER),
            ..GhosttyConfig::default()
        };
        let (style, _warnings) = TerminalStyle::resolve(&config, &ThemeValues::default(), cx);
        style
    });

    let background = window
        .update(cx, |view, _window, cx| {
            view.apply_config(restyled, cx);
            view.session()
                .terminal
                .default_bg_color()
                .expect("the emulator should report its background")
        })
        .expect("the view should restyle");

    assert_eq!(
        background,
        Some(RgbColor {
            r: MARKER.r,
            g: MARKER.g,
            b: MARKER.b
        })
    );
}

#[gpui_kit::test]
fn a_focused_session_in_an_inactive_window_draws_the_hollow_cursor(cx: &mut TestAppContext) {
    let window = open(size(px(640.), px(400.)), cx);
    window
        .update(cx, |view, window, cx| {
            let focus = view.focus_handle(cx);
            window.focus(&focus, cx);
            window.activate_window();
        })
        .expect("the view should take the focus");
    cx.run_until_parked();
    draw(&window, cx);
    let active = focused(&window, cx);

    VisualTestContext::from_window(window.into(), cx).deactivate_window();
    draw(&window, cx);
    let inactive = focused(&window, cx);

    assert!(
        active,
        "a focused session in the active window fills its cursor"
    );
    assert!(
        !inactive,
        "an inactive window hollows it, however the session's own focus stands"
    );
}

/// Whether the session would draw a filled cursor (REQ-15).
fn focused(window: &WindowHandle<TerminalView>, cx: &mut TestAppContext) -> bool {
    window
        .update(cx, |view, _window, _cx| view.state().borrow().focused)
        .expect("the view should report its focus")
}

/// Draw one frame of the window.
fn draw(window: &WindowHandle<TerminalView>, cx: &mut TestAppContext) {
    cx.update_window((*window).into(), |_root, window, cx| {
        window.render_frame(cx)
    })
    .expect("the window should draw");
}

#[gpui_kit::test]
fn a_smaller_area_refits_the_grid(cx: &mut TestAppContext) {
    let window = open(size(px(640.), px(400.)), cx);
    let first = window
        .update(cx, |view, _window, _cx| view.session().size())
        .expect("the view should report its grid");

    window
        .update(cx, |view, window, _cx| {
            view.state()
                .borrow_mut()
                .lay_out(bounds(size(px(320.), px(200.))), window);
        })
        .expect("the view should lay out again");
    let second = window
        .update(cx, |view, _window, _cx| view.session().size())
        .expect("the view should report its grid");

    assert!(second.columns < first.columns, "{second:?} < {first:?}");
    assert!(second.rows < first.rows, "{second:?} < {first:?}");
}
