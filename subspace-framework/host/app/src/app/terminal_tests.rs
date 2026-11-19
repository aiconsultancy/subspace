//! The docked terminal in a real shell window: where it sits for each
//! placement and bottom span, maximize, showing and hiding with focus,
//! stored choices and the quit warnings, with a fixture panel.

use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    App, AppContext as _, Bounds, Context, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, Pixels, Render, Styled as _, TestAppContext, Window,
    WindowHandle, div, px, size,
};
use subspace_chat::OPEN_KEY as CHAT_OPEN_KEY;
use subspace_dashboard::dock_panel::{BottomSpan, Placement};
use subspace_dashboard::{Dashboard, DashboardEvent, prefs};

use super::terminal_dock::Change;
use super::terminal_fixture::{self, PANEL_ID};
use super::{SubspaceApp, ToggleTerminal};
use crate::registry::{BoardFactories, BoardRegistration};

const SPACE_ID: &str = "terminal-test-space";
const CHAT_ID: &str = "agent-chat";
/// Pixel slack for borders and layout rounding.
const TOLERANCE: f32 = 1.5;

struct Space {
    focus: FocusHandle,
}

impl EventEmitter<DashboardEvent> for Space {}

impl Focusable for Space {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Space {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(SPACE_ID)
            .test_support()
            .track_focus(&self.focus)
            .size_full()
    }
}

impl Dashboard for Space {
    const ID: &'static str = "terminal-test-space";
    const TITLE: &'static str = "Terminal test Space";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

struct OtherSpace {
    focus: FocusHandle,
}

impl EventEmitter<DashboardEvent> for OtherSpace {}

impl Focusable for OtherSpace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for OtherSpace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.focus).size_full()
    }
}

impl Dashboard for OtherSpace {
    const ID: &'static str = "terminal-test-other";
    const TITLE: &'static str = "Other Space";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

struct Shell {
    window: WindowHandle<SubspaceApp>,
    root: tempfile::TempDir,
}

impl Shell {
    /// A window with the fixture terminal, the given stored choices applied
    /// before it opens.
    fn open(cx: &mut TestAppContext, stored: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().unwrap();
        Self::open_in(cx, root, stored)
    }

    fn open_in(cx: &mut TestAppContext, root: tempfile::TempDir, stored: &[(&str, &str)]) -> Self {
        let path = root.path().to_path_buf();
        cx.update(|cx| {
            crate::theme::preview_tests::init_app(cx, &path, path.join("state.json"));
            crate::theme::initialize(cx);
            prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
            prefs::set(cx, CHAT_OPEN_KEY, "0");
            for (key, value) in stored {
                prefs::set(cx, key, *value);
            }
            terminal_fixture::install(cx);
            crate::terminal::bind_keys(cx);
            cx.set_global(BoardFactories(vec![
                BoardRegistration::new::<Space>(),
                BoardRegistration::new::<OtherSpace>(),
            ]));
        });
        let window = cx.open_window(size(px(1400.), px(900.)), |window, cx| {
            SubspaceApp::new(Vec::new(), window, cx)
        });
        let shell = Self { window, root };
        shell.settle(cx);
        shell
    }

    fn settle(&self, cx: &mut TestAppContext) {
        for _ in 0..4 {
            cx.update_window(self.window.into(), |_, window, cx| window.render_frame(cx))
                .unwrap();
            cx.run_until_parked();
        }
    }

    fn change(&self, cx: &mut TestAppContext, change: Change) {
        self.window
            .update(cx, |app, window, cx| {
                app.change_terminal(change, window, cx)
            })
            .unwrap();
        self.settle(cx);
    }

    /// Shows the chat pane without starting an agent runtime.
    fn show_chat(&self, cx: &mut TestAppContext) {
        self.window
            .update(cx, |app, _, cx| {
                app.chat_open = true;
                cx.notify();
            })
            .unwrap();
        self.settle(cx);
    }

    fn bounds(&self, cx: &mut TestAppContext, id: &'static str) -> Option<Bounds<Pixels>> {
        cx.update_window(self.window.into(), |_, window, _| {
            window.try_find(id).map(|element| element.bounds())
        })
        .unwrap()
    }

    /// The Space's, the terminal's and the chat's bounds.
    fn parts(&self, cx: &mut TestAppContext) -> [Bounds<Pixels>; 3] {
        [SPACE_ID, PANEL_ID, CHAT_ID].map(|id| {
            self.bounds(cx, id)
                .unwrap_or_else(|| panic!("{id} is drawn"))
        })
    }

    fn stored(&self, cx: &mut TestAppContext, key: &str) -> Option<String> {
        self.window
            .update(cx, |_, _, cx| prefs::get(cx, key))
            .unwrap()
    }
}

fn at_or_before(first: Pixels, second: Pixels) -> bool {
    f32::from(first) <= f32::from(second) + TOLERANCE
}

fn open_at(cx: &mut TestAppContext, placement: Placement, span: BottomSpan) -> Shell {
    let shell = Shell::open(
        cx,
        &[
            (crate::terminal::OPEN_KEY, "1"),
            (crate::terminal::PLACEMENT_KEY, placement.key()),
            (crate::terminal::SPAN_KEY, span.key()),
        ],
    );
    shell.show_chat(cx);
    shell
}

#[gpui_kit::test]
fn a_bottom_terminal_sits_below_the_space_beside_the_full_height_chat(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let shell = open_at(cx, Placement::Bottom, BottomSpan::BesideChat);
    let [space, terminal, chat] = shell.parts(cx);
    assert!(at_or_before(space.bottom(), terminal.top()));
    assert!(at_or_before(terminal.right(), chat.left()));
    assert!(at_or_before(chat.top(), space.top()));
    assert!(at_or_before(terminal.bottom(), chat.bottom()));
    assert!((f32::from(terminal.left()) - f32::from(space.left())).abs() <= TOLERANCE);
}

#[gpui_kit::test]
fn a_full_width_bottom_terminal_spans_below_the_space_and_the_chat(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let shell = open_at(cx, Placement::Bottom, BottomSpan::FullWidth);
    let [space, terminal, chat] = shell.parts(cx);
    assert!(at_or_before(space.bottom(), terminal.top()));
    assert!(at_or_before(chat.bottom(), terminal.top()));
    assert!(at_or_before(terminal.left(), space.left()));
    assert!(at_or_before(chat.right(), terminal.right()));
}

#[gpui_kit::test]
fn a_left_terminal_precedes_the_space_and_the_chat(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let shell = open_at(cx, Placement::Left, BottomSpan::BesideChat);
    let [space, terminal, chat] = shell.parts(cx);
    assert!(at_or_before(terminal.right(), space.left()));
    assert!(at_or_before(space.right(), chat.left()));
}

#[gpui_kit::test]
fn a_right_terminal_sits_between_the_space_and_the_outermost_chat(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let shell = open_at(cx, Placement::Right, BottomSpan::BesideChat);
    let [space, terminal, chat] = shell.parts(cx);
    assert!(at_or_before(space.right(), terminal.left()));
    assert!(at_or_before(terminal.right(), chat.left()));
}

#[gpui_kit::test]
fn maximize_covers_the_space_and_the_chat_until_hidden_or_the_space_changes(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let shell = open_at(cx, Placement::Bottom, BottomSpan::BesideChat);
    let [space, _, chat] = shell.parts(cx);
    shell.change(cx, Change::ToggleMaximized);
    assert!(shell.bounds(cx, SPACE_ID).is_none());
    assert!(shell.bounds(cx, CHAT_ID).is_none());
    let terminal = shell.bounds(cx, PANEL_ID).unwrap();
    assert!(at_or_before(terminal.left(), space.left()));
    assert!(at_or_before(chat.right(), terminal.right()));
    assert!(at_or_before(terminal.top(), space.top()));
    assert_eq!(
        shell.stored(cx, crate::terminal::OPEN_KEY).as_deref(),
        Some("1")
    );

    shell.change(cx, Change::Hide);
    assert!(shell.bounds(cx, PANEL_ID).is_none());
    assert!(shell.bounds(cx, SPACE_ID).is_some());

    shell.change(cx, Change::ToggleMaximized);
    assert!(shell.bounds(cx, SPACE_ID).is_none());
    shell
        .window
        .update(cx, |app, window, cx| app.switch_to(1, window, cx))
        .unwrap();
    shell.settle(cx);
    assert!(shell.bounds(cx, CHAT_ID).is_some());
    assert!(
        shell.bounds(cx, PANEL_ID).is_some(),
        "switching restores, not hides"
    );
}

#[gpui_kit::test]
fn ctrl_backtick_shows_the_terminal_with_focus_and_hides_it_back_to_the_space(
    cx: &mut TestAppContext,
) {
    let shell = Shell::open(cx, &[]);
    let panel = shell
        .window
        .update(cx, |app, _, _| terminal_fixture::panel(app))
        .unwrap();
    assert!(shell.bounds(cx, PANEL_ID).is_none());
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.shown),
        0,
        "starts lazily"
    );
    let press = |cx: &mut TestAppContext| {
        cx.update_window(shell.window.into(), |_, window, cx| {
            window.press("ctrl-`", cx)
        })
        .unwrap();
        shell.settle(cx);
    };
    press(cx);
    assert!(shell.bounds(cx, PANEL_ID).is_some());
    assert_eq!(panel.read_with(cx, |panel, _| panel.shown), 1);
    let focused = |cx: &mut TestAppContext| {
        shell
            .window
            .update(cx, |app, window, cx| app.terminal_focused(window, cx))
            .unwrap()
    };
    assert!(focused(cx));
    press(cx);
    assert!(shell.bounds(cx, PANEL_ID).is_none());
    assert!(!focused(cx));
    let space_focused = shell
        .window
        .update(cx, |app, window, cx| {
            app.dashboard()
                .unwrap()
                .focus_handle(cx)
                .contains_focused(window, cx)
        })
        .unwrap();
    assert!(space_focused);
    assert_eq!(
        shell.stored(cx, crate::terminal::OPEN_KEY).as_deref(),
        Some("0")
    );
    cx.update_window(shell.window.into(), |_, window, cx| {
        window.dispatch_action(Box::new(ToggleTerminal), cx);
    })
    .unwrap();
    shell.settle(cx);
    assert!(focused(cx));
}

mod actions;
