//! The shell's last view: what a board is told it was reached
//! from, and where a Back's `Return` lands.

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, Global, IntoElement, Render, SharedString,
    TestAppContext, Window, WindowHandle, div,
};
use subspace_dashboard::{Dashboard, DashboardEvent, ReturnLocation, ReturnPoint, prefs};

use super::SubspaceApp;

#[derive(Default)]
struct Seen {
    /// The place the channel-like board reports, by its key.
    location: Option<&'static str>,
    /// Whether that place still exists.
    location_open: bool,
    /// Keys the channel-like board was asked to show.
    returned_to: Vec<String>,
    /// What the issue-like board was told it was reached from.
    arrivals: Vec<Option<ReturnPoint>>,
}
impl Global for Seen {}

macro_rules! fixture {
    ($name:ident) => {
        struct $name {
            focus: FocusHandle,
        }
        impl EventEmitter<DashboardEvent> for $name {}
        impl Focusable for $name {
            fn focus_handle(&self, _: &App) -> FocusHandle {
                self.focus.clone()
            }
        }
        impl Render for $name {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
            }
        }
    };
}

fixture!(Channels);
fixture!(Tracker);
fixture!(Other);

impl Dashboard for Channels {
    const ID: &'static str = "irc";
    const TITLE: &'static str = "Channels";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn return_location(&self, cx: &App) -> Option<ReturnLocation> {
        cx.global::<Seen>().location.map(|key| ReturnLocation {
            label: SharedString::from(format!("#{key}")),
            key: key.into(),
        })
    }
    fn return_to(&mut self, key: &str, _: &mut Window, cx: &mut Context<Self>) -> bool {
        let seen = cx.global_mut::<Seen>();
        seen.returned_to.push(key.to_owned());
        seen.location_open
    }
}

impl Dashboard for Tracker {
    const ID: &'static str = "issues";
    const TITLE: &'static str = "Issues";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn arrived_from(&mut self, from: Option<ReturnPoint>, cx: &mut Context<Self>) {
        cx.global_mut::<Seen>().arrivals.push(from);
    }
}

impl Dashboard for Other {
    const ID: &'static str = "other";
    const TITLE: &'static str = "PRs";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

const CHANNELS: usize = 0;
const ISSUES: usize = 1;
const OTHER: usize = 2;

fn open(cx: &mut TestAppContext) -> (WindowHandle<SubspaceApp>, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        cx.set_global(Seen {
            location: Some("discovery"),
            location_open: true,
            ..Seen::default()
        });
        cx.set_global(crate::registry::BoardFactories(vec![
            crate::registry::BoardRegistration::new::<Channels>(),
            crate::registry::BoardRegistration::new::<Tracker>(),
            crate::registry::BoardRegistration::new::<Other>(),
        ]));
        crate::notifications::init(cx);
    });
    let window = cx.add_window(|window, cx| SubspaceApp::new(Vec::new(), window, cx));
    window
        .update(cx, |app, window, cx| {
            app.switch_to(CHANNELS, window, cx);
            cx.global_mut::<Seen>().arrivals.clear();
        })
        .unwrap();
    (window, root)
}

fn channel_point() -> ReturnPoint {
    ReturnPoint {
        board: "irc",
        label: "#discovery".into(),
        location: Some("discovery".into()),
    }
}

fn toasts(app: &SubspaceApp) -> Vec<String> {
    app.toasts
        .iter()
        .map(|shown| shown.toast.text.to_string())
        .collect()
}

#[gpui_kit::test]
fn switching_space_tells_the_next_board_the_view_it_came_from(cx: &mut TestAppContext) {
    let (window, _root) = open(cx);
    window
        .update(cx, |app, window, cx| {
            app.switch_to(ISSUES, window, cx);
            assert_eq!(cx.global::<Seen>().arrivals, [Some(channel_point())]);
            assert_eq!(app.last_view, Some(channel_point()));
            // Showing the current board again records nothing.
            app.switch_to(ISSUES, window, cx);
            assert_eq!(cx.global::<Seen>().arrivals.len(), 1);
            app.switch_to(OTHER, window, cx);
            app.switch_to(ISSUES, window, cx);
            let other = ReturnPoint {
                board: "other",
                label: "PRs".into(),
                location: None,
            };
            assert_eq!(cx.global::<Seen>().arrivals.last(), Some(&Some(other)));
            assert_eq!(app.last_view.as_ref().map(|view| view.board), Some("other"));
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_board_link_carries_the_requesters_view(cx: &mut TestAppContext) {
    let (window, _root) = open(cx);
    window
        .update(cx, |app, window, cx| {
            app.note_arrival(ISSUES, cx);
            assert_eq!(cx.global::<Seen>().arrivals, [Some(channel_point())]);
            // The board already showing is not told anything.
            app.note_arrival(CHANNELS, cx);
            assert_eq!(cx.global::<Seen>().arrivals.len(), 1);
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_board_link_from_a_named_place_carries_that_place(cx: &mut TestAppContext) {
    let (window, _root) = open(cx);
    window
        .update(cx, |app, window, cx| {
            // The board focuses #discovery, but the link came from #ops.
            let ops = ReturnLocation {
                key: "ops".into(),
                label: "#ops".into(),
            };
            app.note_arrival_from(ISSUES, Some(ops), cx);
            let expected = ReturnPoint {
                board: "irc",
                label: "#ops".into(),
                location: Some("ops".into()),
            };
            assert_eq!(cx.global::<Seen>().arrivals, [Some(expected.clone())]);
            assert_eq!(app.last_view, Some(expected));
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn return_shows_the_origin_board_and_place(cx: &mut TestAppContext) {
    let (window, _root) = open(cx);
    window
        .update(cx, |app, window, cx| {
            app.switch_to(ISSUES, window, cx);
            app.return_to_view("issues", &channel_point(), window, cx);
            assert_eq!(app.active, CHANNELS);
            assert_eq!(cx.global::<Seen>().returned_to, ["discovery"]);
            assert!(toasts(app).is_empty());
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_return_from_a_board_not_showing_is_ignored(cx: &mut TestAppContext) {
    let (window, _root) = open(cx);
    window
        .update(cx, |app, window, cx| {
            app.switch_to(OTHER, window, cx);
            app.return_to_view("issues", &channel_point(), window, cx);
            assert_eq!(app.active, OTHER);
            assert!(cx.global::<Seen>().returned_to.is_empty());
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_closed_place_keeps_the_requester_and_says_so(cx: &mut TestAppContext) {
    let (window, _root) = open(cx);
    window
        .update(cx, |app, window, cx| {
            app.switch_to(ISSUES, window, cx);
            cx.global_mut::<Seen>().location_open = false;
            app.return_to_view("issues", &channel_point(), window, cx);
            assert_eq!(app.active, ISSUES);
            assert_eq!(toasts(app), ["#discovery is no longer open."]);
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_missing_board_keeps_the_requester_and_says_so(cx: &mut TestAppContext) {
    let (window, _root) = open(cx);
    window
        .update(cx, |app, window, cx| {
            app.switch_to(ISSUES, window, cx);
            let gone = ReturnPoint {
                board: "gone",
                label: "Builds".into(),
                location: None,
            };
            app.return_to_view("issues", &gone, window, cx);
            assert_eq!(app.active, ISSUES);
            assert_eq!(toasts(app), ["Builds is closed."]);
            window.remove_window();
        })
        .unwrap();
}
