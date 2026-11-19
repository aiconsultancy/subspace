use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, Render, Styled as _, TestAppContext, Window,
    WindowOptions, div,
};
use subspace_dashboard::{Dashboard, DashboardEvent, KeyHelp, prefs};

use super::super::{SubspaceApp, bind_keys};
use crate::registry::{BoardFactories, BoardRegistration};

struct Board {
    focus: FocusHandle,
}
impl EventEmitter<DashboardEvent> for Board {}
impl Focusable for Board {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Board {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("shortcuts-test-board")
            .track_focus(&self.focus)
            .size_full()
    }
}
impl Dashboard for Board {
    const ID: &'static str = "shortcuts-test";
    const TITLE: &'static str = "Shortcuts test";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn keys(&self) -> &'static [KeyHelp] {
        &[KeyHelp {
            key: "z",
            meaning: "Fixture action",
        }]
    }
}

struct OtherBoard {
    focus: FocusHandle,
}
impl EventEmitter<DashboardEvent> for OtherBoard {}
impl Focusable for OtherBoard {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for OtherBoard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.focus).size_full()
    }
}
impl Dashboard for OtherBoard {
    const ID: &'static str = "shortcuts-other";
    const TITLE: &'static str = "Other board";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<SubspaceApp>, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.json"), "{}").unwrap();
    let (window, app) = cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, subspace_chat::OPEN_KEY, "0");
        cx.set_global(BoardFactories(vec![
            BoardRegistration::new::<Board>(),
            BoardRegistration::new::<OtherBoard>(),
        ]));
        bind_keys(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SubspaceApp::new(Vec::new(), window, cx))
        })
        .unwrap()
    });
    cx.run_until_parked();
    (window, app, root)
}

#[gpui_kit::test]
fn filtering_and_board_switch_close_the_current_snapshot(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press("secondary-?", cx);
        window.render_frame(cx);
        assert!(window.find("shortcuts-key-z-Fixture action").visible());
        window.input("fixture action", cx);
        assert!(window.find("shortcuts-key-z-Fixture action").visible());
        assert!(
            window
                .try_find("shortcuts-key-w-Wrap long titles")
                .is_none()
        );
        window.press("secondary-2", cx);
        assert_eq!(app.read(cx).dashboard().unwrap().id, "shortcuts-other");
        assert!(!app.read(cx).shortcuts_anchor.read(cx).is_open());
    })
    .unwrap();
}

#[gpui_kit::test]
fn shortcut_popup_preserves_open_help(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("help", cx);
        assert!(app.read(cx).help_open);
        window.press("secondary-?", cx);
        window.render_frame(cx);
        assert!(app.read(cx).shortcuts_anchor.read(cx).is_open());
        assert!(app.read(cx).help_open);
        window.press("secondary-?", cx);
        assert!(!app.read(cx).shortcuts_anchor.read(cx).is_open());
        assert!(app.read(cx).help_open);
    })
    .unwrap();
}

#[gpui_kit::test]
fn keyboard_shortcuts_toggle_and_escape_restore_origin(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    let origin = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let origin = window.focused(cx).unwrap();
            window.press("secondary-shift-/", cx);
            window.render_frame(cx);
            assert!(app.read(cx).shortcuts_anchor.read(cx).is_open());
            assert!(!app.read(cx).help_open);
            window.press("escape", cx);
            assert!(!app.read(cx).shortcuts_anchor.read(cx).is_open());
            origin
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, _| assert!(origin.is_focused(window)))
        .unwrap();
}

#[gpui_kit::test]
fn palette_keyboard_shortcuts_command_opens_the_named_view(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let palette = if cfg!(target_os = "linux") {
            "ctrl-shift-k"
        } else {
            "secondary-k"
        };
        window.press(palette, cx);
        window.input("Keyboard Shortcuts", cx);
        window.render_frame(cx);
        assert!(app.read(cx).session.is_some());
        let session = app.read(cx).session.as_ref().unwrap();
        let commands: Vec<_> = session
            .sections
            .iter()
            .flat_map(|section| &section.entries)
            .collect();
        assert!(commands.iter().any(|entry| matches!(entry,
            crate::palette::Entry::Command(command) if command.label == "Keyboard Shortcuts"
        )));
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(app.read(cx).session.is_none());
        assert!(app.read(cx).shortcuts_anchor.read(cx).is_open());
        assert_eq!(
            window.find("keyboard-shortcuts").label(),
            Some("Keyboard Shortcuts")
        );
        assert_eq!(
            window.find("keyboard-shortcuts").role(),
            Some(gpui_kit::Role::Dialog)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn opening_settings_dismisses_shortcuts_instead_of_stacking_surfaces(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press("secondary-?", cx);
        window.render_frame(cx);
        assert!(app.read(cx).shortcuts_anchor.read(cx).is_open());
        app.update(cx, |app, cx| {
            app.open_settings(&super::super::OpenSettings, window, cx)
        });
        assert!(app.read(cx).settings.is_some());
        assert!(!app.read(cx).shortcuts_anchor.read(cx).is_open());
    })
    .unwrap();
}
