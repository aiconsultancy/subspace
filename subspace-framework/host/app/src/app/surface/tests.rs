use std::path::Path;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, ElementId, EventEmitter, FocusHandle, Focusable, Global,
    IntoElement, Render, TestAppContext, Window, WindowHandle, div,
};
use subspace_agent_tools::Catalog;
use subspace_core::{config::Config, credentials::Credentials};
use subspace_dashboard::density::Density;
use subspace_dashboard::log_view::OpenLogView;
use subspace_dashboard::{Dashboard, DashboardEvent, Services};

use super::MainSurface;
use crate::app::{
    CloseBoardHelp, CloseLogView, CloseSearchView, OpenSearchView, SubspaceApp, ToggleBoardHelp,
};

/// How often the board heard each lifecycle call.
#[derive(Default)]
struct Calls {
    activated: usize,
    deactivated: usize,
}
impl Global for Calls {}

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
    }
}
impl Dashboard for Board {
    const ID: &'static str = "surface-fixture";
    const TITLE: &'static str = "Surface fixture";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn activate(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.global_mut::<Calls>().activated += 1;
    }
    fn deactivate(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.global_mut::<Calls>().deactivated += 1;
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

fn open(root: &Path, cx: &mut TestAppContext) -> WindowHandle<SubspaceApp> {
    let search = subspace_search::Service::open(
        root.join("search"),
        "test-app".into(),
        vec![Board::ID.into()],
    )
    .unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        let config = Config::default();
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&config)),
            config: Arc::new(config),
            tools: Catalog::new(Vec::new()).unwrap(),
            search: Some(search),
            config_file: root.join("config.json"),
        });
        cx.set_global(Calls::default());
        cx.set_global(crate::registry::BoardFactories(vec![
            crate::registry::BoardRegistration::new::<Board>(),
        ]));
    });
    let window = cx.add_window(|window, cx| SubspaceApp::new(Vec::new(), window, cx));
    cx.run_until_parked();
    cx.update(|cx| cx.set_global(Calls::default()));
    window
}

fn calls(cx: &TestAppContext) -> (usize, usize) {
    cx.read(|cx| {
        let calls = cx.global::<Calls>();
        (calls.activated, calls.deactivated)
    })
}

fn update(
    window: WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
    change: impl FnOnce(&mut SubspaceApp, &mut Window, &mut Context<SubspaceApp>),
) {
    window.update(cx, change).unwrap();
    cx.run_until_parked();
}

fn showing(window: WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> Option<&'static str> {
    window.read_with(cx, |app, _| app.main.title()).unwrap()
}

#[gpui_kit::test]
fn search_replaces_logs_and_the_board_returns_active_exactly_once(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open(root.path(), cx);

    update(window, cx, |app, window, cx| {
        app.open_log_view(&OpenLogView, window, cx)
    });
    assert_eq!(showing(window, cx), Some("Logs"));
    assert_eq!(calls(cx), (0, 1));

    // Closing Search while Logs shows must not wake the board underneath.
    update(window, cx, |app, window, cx| {
        app.close_full_text(&CloseSearchView, window, cx)
    });
    assert_eq!(showing(window, cx), Some("Logs"));
    assert_eq!(calls(cx), (0, 1));

    update(window, cx, |app, window, cx| {
        app.open_full_text(&OpenSearchView, window, cx)
    });
    assert_eq!(showing(window, cx), Some("Search"));
    assert_eq!(calls(cx), (0, 1), "the board is already inactive");

    update(window, cx, |app, window, cx| {
        app.close_full_text(&CloseSearchView, window, cx)
    });
    assert_eq!(showing(window, cx), None);
    assert_eq!(calls(cx), (1, 1));

    update(window, cx, |app, window, cx| {
        app.close_log_view(&CloseLogView, window, cx)
    });
    assert!(window.read_with(cx, |app, _| app.main.is_board()).unwrap());
    assert_eq!(calls(cx), (1, 1), "Logs was already replaced");
}

#[gpui_kit::test]
fn switching_boards_from_a_view_does_not_deactivate_the_hidden_board_again(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let window = open(root.path(), cx);
    update(window, cx, |app, window, cx| {
        app.open_log_view(&OpenLogView, window, cx)
    });
    update(window, cx, |app, window, cx| {
        let active = app.active;
        app.switch_to(active, window, cx);
    });
    assert_eq!(showing(window, cx), None);
    assert_eq!(calls(cx), (1, 1));
}

/// Draw the window, then whether the rail shows its only board as current.
fn rail_current(window: WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> bool {
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(("vertical-dashboard-tab", 0usize)).visible());
    })
    .unwrap();
    window.read_with(cx, |app, _| app.shows_current(0)).unwrap()
}

fn click(window: WindowHandle<SubspaceApp>, id: impl Into<ElementId>, cx: &mut TestAppContext) {
    cx.update_window(window.into(), |_, window, cx| window.click(id, cx))
        .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_logs_header_and_the_rail_both_return_to_the_board(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open(root.path(), cx);
    update(window, cx, |app, window, cx| {
        app.toggle_navigation_mode(window, cx)
    });
    assert!(rail_current(window, cx));

    update(window, cx, |app, window, cx| {
        app.open_log_view(&OpenLogView, window, cx)
    });
    assert!(!rail_current(window, cx), "no board is current");
    click(window, "surface-close", cx);
    assert_eq!(showing(window, cx), None);
    assert!(rail_current(window, cx));
    assert_eq!(calls(cx), (1, 1));

    update(window, cx, |app, window, cx| {
        app.open_log_view(&OpenLogView, window, cx)
    });
    assert!(!rail_current(window, cx));
    click(window, ("vertical-dashboard-tab", 0usize), cx);
    assert_eq!(showing(window, cx), None);
    assert!(rail_current(window, cx));
    assert_eq!(calls(cx), (2, 2));
}

fn preview_density(window: WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> Density {
    let before = cx.read(|cx| cx.global::<Density>().clone());
    update(window, cx, |app, window, cx| {
        app.set_jig(true, window, cx);
        cx.global_mut::<Density>().set("--row-h", 41.0);
    });
    assert!(window.read_with(cx, |app, _| app.jig_open.get()).unwrap());
    assert_ne!(cx.read(|cx| cx.global::<Density>().clone()), before);
    before
}

fn assert_preview_cancelled(
    window: WindowHandle<SubspaceApp>,
    before: &Density,
    cx: &mut TestAppContext,
) {
    assert!(!window.read_with(cx, |app, _| app.jig_open.get()).unwrap());
    assert_eq!(&cx.read(|cx| cx.global::<Density>().clone()), before);
}

#[gpui_kit::test]
fn opening_logs_or_search_cancels_the_theme_designer_preview(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open(root.path(), cx);

    let before = preview_density(window, cx);
    update(window, cx, |app, window, cx| {
        app.open_log_view(&OpenLogView, window, cx)
    });
    assert_eq!(showing(window, cx), Some("Logs"));
    assert_preview_cancelled(window, &before, cx);

    let before = preview_density(window, cx);
    update(window, cx, |app, window, cx| {
        app.open_full_text(&OpenSearchView, window, cx)
    });
    assert_eq!(showing(window, cx), Some("Search"));
    assert_preview_cancelled(window, &before, cx);
}

#[gpui_kit::test]
fn board_help_commands_drive_the_help_dock(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open(root.path(), cx);
    update(window, cx, |app, window, cx| {
        app.toggle_board_help(&ToggleBoardHelp, window, cx)
    });
    assert!(window.read_with(cx, |app, _| app.help_open).unwrap());
    update(window, cx, |app, window, cx| {
        app.close_board_help(&CloseBoardHelp, window, cx)
    });
    assert!(!window.read_with(cx, |app, _| app.help_open).unwrap());
}

#[test]
fn only_the_board_has_no_view_or_title() {
    assert!(MainSurface::Board.is_board());
    assert!(MainSurface::Board.title().is_none());
    assert!(MainSurface::Board.view().is_none());
    assert!(MainSurface::Board.search().is_none());
    assert!(!MainSurface::Board.is_logs());
}

#[path = "tests/find.rs"]
mod find_commands;
