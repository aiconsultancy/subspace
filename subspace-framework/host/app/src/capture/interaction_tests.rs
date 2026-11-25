//! Capture dispatch with the real shared chat composer focused, offline.
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Global, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render,
    TestAppContext, Window, WindowOptions, actions, div,
};
use subspace_dashboard::{ChatInput, Dashboard, DashboardEvent, prefs};

use super::{dispatch, open_link, press, press_space};
use crate::app::SubspaceApp;
use crate::registry::{BoardFactories, BoardRegistration};

actions!(capture_test, [MoveDown, Unhandled]);

struct Fixture(Entity<Space>);
impl Global for Fixture {}

struct Space {
    focus: FocusHandle,
    composer: Entity<ChatInput>,
    down: usize,
}

impl EventEmitter<DashboardEvent> for Space {}
impl Focusable for Space {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Dashboard for Space {
    const ID: &'static str = "capture-test";
    const TITLE: &'static str = "Capture test";
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let entity = cx.entity();
        cx.set_global(Fixture(entity));
        Self {
            focus: cx.focus_handle(),
            composer: cx.new(|cx| ChatInput::message(window, cx)),
            down: 0,
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}
impl Render for Space {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .key_context("CaptureTest")
            .on_action(cx.listener(|space, _: &MoveDown, _, _| space.down += 1))
            .child(self.composer.clone())
    }
}

const LINK: &str = "example://item?tags=a,b";

struct LinkSpace(FocusHandle);

impl EventEmitter<DashboardEvent> for LinkSpace {}
impl Focusable for LinkSpace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.0.clone()
    }
}
impl Dashboard for LinkSpace {
    const ID: &'static str = "capture-link";
    const TITLE: &'static str = "Capture link";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self(cx.focus_handle())
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn open_link(&mut self, url: &str, _: &mut Context<Self>) -> bool {
        url == LINK
    }
}
impl Render for LinkSpace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.0)
    }
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Space>, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, subspace_chat::OPEN_KEY, "0");
        prefs::set(cx, "app.dashboard", Space::ID);
        cx.set_global(BoardFactories(vec![
            BoardRegistration::new::<Space>(),
            BoardRegistration::new::<LinkSpace>(),
        ]));
        cx.bind_keys([KeyBinding::new("down", MoveDown, Some("CaptureTest"))]);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SubspaceApp::new(Vec::new(), window, cx))
        })
        .unwrap()
    });
    cx.run_until_parked();
    let space = cx.update(|cx| cx.global::<Fixture>().0.clone());
    cx.update_window(window, |_, window, cx| {
        let composer = space.read(cx).composer.clone();
        composer.update(cx, |input, cx| input.focus(window, cx));
        window.draw(cx).clear(cx);
    })
    .unwrap();
    (window, space, root)
}

#[gpui_kit::test]
fn a_capture_link_switches_only_when_the_named_space_accepts_it(cx: &mut TestAppContext) {
    let (_, _, _root) = open(cx);
    cx.update(|cx| {
        let unknown = open_link("missing-space", LINK, cx).unwrap_err();
        assert_eq!(unknown, "link: unknown Space missing-space");
        let refused = open_link(LinkSpace::ID, "example://unknown", cx).unwrap_err();
        assert_eq!(refused, "link: Space capture-link refused the link");
        assert_eq!(prefs::get(cx, "app.dashboard").as_deref(), Some(Space::ID));
        open_link(LinkSpace::ID, LINK, cx).unwrap();
        assert_eq!(
            prefs::get(cx, "app.dashboard").as_deref(),
            Some(LinkSpace::ID)
        );
    });
}

#[gpui_kit::test]
fn root_action_runs_with_the_chat_composer_focused(cx: &mut TestAppContext) {
    let (window, _, _root) = open(cx);
    let focused = cx
        .update_window(window, |_, window, cx| window.focused(cx))
        .unwrap();
    cx.update(|cx| {
        assert_eq!(
            prefs::get(cx, subspace_chat::OPEN_KEY).as_deref(),
            Some("0")
        );
        dispatch("subspace_app::ToggleChat", cx).unwrap();
        assert_eq!(
            prefs::get(cx, subspace_chat::OPEN_KEY).as_deref(),
            Some("1")
        );
    });
    cx.update_window(window, |_, window, cx| {
        assert_eq!(window.focused(cx), focused, "dispatch does not steal focus");
    })
    .unwrap();
}

#[gpui_kit::test]
fn unknown_and_registered_but_unhandled_actions_report_their_names(cx: &mut TestAppContext) {
    let (_, _, _root) = open(cx);
    cx.update(|cx| {
        for name in [
            "capture_test::Missing",
            "capture_test::Unhandled",
            "capture_test::MoveDown",
        ] {
            let error = dispatch(name, cx).unwrap_err();
            assert!(error.contains(name), "{error}");
        }
    });
}

#[gpui_kit::test]
fn key_steps_focus_the_space_while_typing_keeps_the_composer_target(cx: &mut TestAppContext) {
    let (window, space, _root) = open(cx);
    cx.update(|cx| {
        press("draft".chars().map(String::from), cx).unwrap();
        assert_eq!(space.read(cx).composer.read(cx).text(cx), "draft");
        press_space("down", cx).unwrap();
        assert_eq!(space.read(cx).down, 1);
        assert_eq!(space.read(cx).composer.read(cx).text(cx), "draft");
    });
    cx.update_window(window, |_, window, cx| {
        assert!(space.read(cx).focus.is_focused(window));
    })
    .unwrap();
}
