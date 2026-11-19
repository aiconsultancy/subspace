use gpui_kit::component::dock::{
    BasePanel, DockArea, DockLayout, DockPlacement, DockSkin, Panel, PanelEvent, panel_handle,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, TestSupportExt as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use subspace_dashboard::{Dashboard, DashboardEvent, KeyHelp, prefs};

use super::super::{SubspaceApp, ToggleHelp};
use crate::registry::{BoardFactories, BoardRegistration};

#[path = "review_tests.rs"]
mod review;
mod sizing;

struct Alpha {
    focus: FocusHandle,
    dock: Entity<DockArea>,
}
struct Beta {
    focus: FocusHandle,
    dock: Entity<DockArea>,
}

macro_rules! fixture_board {
    ($name:ident, $id:literal, $title:literal, $doc:literal, $key:literal, $action:literal) => {
        impl EventEmitter<DashboardEvent> for $name {}
        impl Focusable for $name {
            fn focus_handle(&self, _: &App) -> FocusHandle {
                self.focus.clone()
            }
        }
        impl Render for $name {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .id($id)
                    .test_support()
                    .track_focus(&self.focus)
                    .size_full()
                    .child(self.dock.clone())
            }
        }
        impl Dashboard for $name {
            const ID: &'static str = $id;
            const TITLE: &'static str = $title;
            const HELP_MARKDOWN: &'static str = $doc;
            const RETAIN_WHEN_DISABLED: bool = true;
            fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
                Self {
                    focus: cx.focus_handle(),
                    dock: nested_dock($id, window, cx),
                }
            }
            fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
            fn keys(&self) -> &'static [KeyHelp] {
                &[KeyHelp {
                    key: $key,
                    meaning: $action,
                }]
            }
        }
    };
}

struct NativeWorkspace {
    focus: FocusHandle,
}
impl EventEmitter<PanelEvent> for NativeWorkspace {}
impl Focusable for NativeWorkspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for NativeWorkspace {
    fn panel_name(&self) -> &'static str {
        "test-only-native-workspace"
    }
    fn closable(&self, _: &App) -> bool {
        false
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for NativeWorkspace {
    fn title_bar(&self, _: &App) -> bool {
        false
    }
}
impl Render for NativeWorkspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child("Retained native workspace")
    }
}

fn nested_dock(id: &'static str, window: &mut Window, cx: &mut App) -> Entity<DockArea> {
    let (area, _) = DockSkin::dock_area(id, None, window, cx);
    let center = cx.new(|cx| NativeWorkspace {
        focus: cx.focus_handle(),
    });
    let side = cx.new(|cx| NativeWorkspace {
        focus: cx.focus_handle(),
    });
    let center = DockLayout::tabs().panel_view(panel_handle(center), cx);
    let side = DockLayout::tabs().panel_view(panel_handle(side), cx);
    area.update(cx, |area, cx| {
        area.set_center(center, window, cx);
        area.set_dock(DockPlacement::Left, side, window, cx);
        area.set_dock_size(DockPlacement::Left, px(210.), window, cx);
        area.set_locked(true, window, cx);
    });
    area
}

fixture_board!(
    Alpha,
    "help-alpha",
    "Alpha board",
    "# Alpha guide",
    "a",
    "Alpha action"
);
fixture_board!(
    Beta,
    "help-beta",
    "Beta board",
    "# Beta guide",
    "b",
    "Beta action"
);

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<SubspaceApp>, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        crate::theme::preview_tests::nominal_controls(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, subspace_chat::OPEN_KEY, "0");
        cx.set_global(BoardFactories(vec![
            BoardRegistration::new::<Alpha>(),
            BoardRegistration::new::<Beta>(),
        ]));
        super::super::bind_keys(cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1_400.), px(900.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| SubspaceApp::new(Vec::new(), window, cx)),
        )
        .unwrap()
    });
    cx.run_until_parked();
    (window, app, root)
}

#[gpui_kit::test]
fn help_tracks_switches_disabled_active_board_fallback_and_empty_state(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("help", cx);
        assert_eq!(
            window.find("help-dock-board-title").label(),
            Some("Alpha board")
        );
        assert!(window.find("help-key-a-Alpha action").visible());
        assert!(window.try_find("help-key-b-Beta action").is_none());
    })
    .unwrap();
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(super::super::keys::ShowDashboard { index: 1 }), cx);
    })
    .unwrap();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("help-dock-board-title").label(),
            Some("Beta board")
        );
        assert!(window.find("help-key-b-Beta action").visible());
        assert!(window.try_find("help-key-a-Alpha action").is_none());
        cx.set_global(BoardFactories(vec![BoardRegistration::new::<Alpha>()]));
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("help-dock-board-title").label(),
            Some("Alpha board")
        );
        assert!(window.try_find("help-key-b-Beta action").is_none());
        cx.set_global(BoardFactories(Vec::new()));
        assert_eq!(app.read(cx).parked.len(), 1);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("help-dock-board-title").label(), Some("Help"));
        assert!(window.try_find("help-key-a-Alpha action").is_none());
        assert!(window.find("help-key-?-Show or hide Help").label() == Some("Show or hide Help"));
        assert!(app.read(cx).help_open);
    })
    .unwrap();
}

#[gpui_kit::test]
fn close_then_reopen_before_deferred_focus_keeps_help_focus(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.toggle_help(&ToggleHelp, window, cx);
            app.close_help(&super::super::CloseHelp, window, cx);
            app.toggle_help(&ToggleHelp, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(app.read(cx).help_open);
        assert_eq!(window.find("help-dock").focused(), Some(true));
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!app.read_with(cx, |app, _| app.help_open));
}

#[gpui_kit::test]
fn help_movement_and_zoom_preserve_the_boards_retained_native_dock(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    let nested = app.read_with(cx, |app, cx| {
        app.dashboard()
            .unwrap()
            .view()
            .downcast::<Alpha>()
            .unwrap()
            .read(cx)
            .dock
            .clone()
    });
    let before = nested.read_with(cx, |area, cx| area.dump(cx));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("help", cx);
        window.click("menu", cx);
        window.press("down", cx);
        window.press("enter", cx);
        window.click("zoom-in", cx);
        window.click("zoom-out", cx);
        window.click("help-dock-close", cx);
    })
    .unwrap();
    cx.run_until_parked();
    nested.read_with(cx, |area, cx| {
        assert_eq!(area.dump(cx), before);
        assert!(!area.is_zoomed());
        assert_eq!(area.dock_size(DockPlacement::Left), Some(px(210.)));
    });
    app.read_with(cx, |app, cx| {
        let board = app.dashboard().unwrap().view().downcast::<Alpha>().unwrap();
        assert_eq!(board.read(cx).dock, nested);
    });
}

#[gpui_kit::test]
fn closing_help_for_the_palette_does_not_steal_the_palette_focus(cx: &mut TestAppContext) {
    let (window, app, _root) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("help", cx);
        window.click("open-palette", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let app = app.read(cx);
        assert!(app.session.is_some());
        assert!(!app.help_open);
        assert!(app.palette.focus_handle(cx).contains_focused(window, cx));
    })
    .unwrap();
}
