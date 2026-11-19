//! The rendered dock extent follows its parent workspace: a stored size that
//! does not fit leaves the main workspace its share, and the stored size
//! returns when the window grows again.

use std::sync::Arc;

use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, Render, Styled as _, TestAppContext, Window,
    WindowHandle, div, px, size,
};
use subspace_agent_tools::Catalog;
use subspace_chat::OPEN_KEY;
use subspace_core::config::Config;
use subspace_core::credentials::Credentials;
use subspace_dashboard::dock_panel::Placement;
use subspace_dashboard::{Dashboard, DashboardEvent, Services, prefs};

use super::MAX_DOCK_SHARE;
use crate::app::SubspaceApp;
use crate::registry::{BoardFactories, BoardRegistration};

mod minimum;

const MAIN_ID: &str = "dock-extent-main";
const BOTTOM_DOCK_ID: &str = "workspace-dock-bottom";
const SIDE_DOCK_ID: &str = "workspace-dock-side";
/// Stored sizes inside the shell's limits, so they are restored unchanged.
const PREFERRED_BOTTOM: f32 = 300.;
const PREFERRED_SIDE: f32 = 500.;
/// Pixel slack for borders and layout rounding.
const TOLERANCE: f32 = 1.5;

/// A board that fills the main workspace beside the docked terminal.
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
            .id(MAIN_ID)
            .test_support()
            .track_focus(&self.focus)
            .size_full()
    }
}

impl Dashboard for Board {
    const ID: &'static str = "dock-extent";
    const TITLE: &'static str = "Dock extent";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

struct Shell {
    window: WindowHandle<SubspaceApp>,
    app: Entity<SubspaceApp>,
    _root: tempfile::TempDir,
}

impl Shell {
    fn open(cx: &mut TestAppContext, placement: Placement, width: f32, height: f32) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().to_path_buf();
        cx.update(|cx| {
            crate::theme::preview_tests::init_app(cx, &path, path.join("state.json"));
            crate::theme::initialize(cx);
            prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
            prefs::set(cx, OPEN_KEY, "0");
            prefs::set(cx, "app.dock-bottom", PREFERRED_BOTTOM.to_string());
            prefs::set(cx, "app.dock-side", PREFERRED_SIDE.to_string());
            prefs::set(cx, crate::terminal::OPEN_KEY, "1");
            prefs::set(cx, crate::terminal::PLACEMENT_KEY, placement.key());
            crate::app::terminal_fixture::install(cx);
            cx.set_global(BoardFactories(vec![BoardRegistration::new::<Board>()]));
            let config = Config::default();
            cx.set_global(Services {
                credentials: Arc::new(Credentials::from_system(&config)),
                config: Arc::new(config),
                tools: Catalog::new(Vec::new()).unwrap(),
                search: None,
                config_file: path.join("config.json"),
            });
        });
        let window = cx.open_window(size(px(width), px(height)), |window, cx| {
            SubspaceApp::new(Vec::new(), window, cx)
        });
        let app = window.update(cx, |_, _, cx| cx.entity()).unwrap();
        let shell = Self {
            window,
            app,
            _root: root,
        };
        shell.settle(cx);
        shell
    }

    fn settle(&self, cx: &mut TestAppContext) {
        for _ in 0..6 {
            cx.update_window(self.window.into(), |_, window, cx| window.render_frame(cx))
                .unwrap();
            cx.run_until_parked();
        }
    }

    fn resize(&self, cx: &mut TestAppContext, width: f32, height: f32) {
        cx.simulate_window_resize(self.window.into(), size(px(width), px(height)));
        self.settle(cx);
    }

    /// The dock's and the main workspace's extent along the dock's axis:
    /// height for the bottom dock, width for a side dock.
    fn extents(&self, cx: &mut TestAppContext, dock_id: &'static str) -> (f32, f32) {
        cx.update_window(self.window.into(), |_, window, _| {
            let dock = window.find(dock_id).bounds().size;
            let main = window.find(MAIN_ID).bounds().size;
            if dock_id == BOTTOM_DOCK_ID {
                (f32::from(dock.height), f32::from(main.height))
            } else {
                (f32::from(dock.width), f32::from(main.width))
            }
        })
        .unwrap()
    }

    /// The stored `(bottom, side)` sizes.
    fn preferred(&self, cx: &TestAppContext) -> (f32, f32) {
        self.app
            .read_with(cx, |app, _| (app.dock_bottom, app.dock_side))
    }
}

fn assert_extent(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= TOLERANCE,
        "{what}: {actual} should be {expected}"
    );
}

/// The dock is capped below the stored size and leaves the main workspace its share.
fn assert_constrained(dock: f32, main: f32, preferred: f32) {
    let parent = dock + main;
    assert!(dock > 0., "the dock stays visible");
    assert!(
        dock < preferred - TOLERANCE,
        "the dock {dock} must be smaller than the stored {preferred}"
    );
    assert!(
        dock <= parent * MAX_DOCK_SHARE + TOLERANCE,
        "the dock {dock} covers more than its share of {parent}"
    );
    assert!(
        main >= parent * (1. - MAX_DOCK_SHARE) - TOLERANCE,
        "the main workspace {main} lost its share of {parent}"
    );
}

#[gpui_kit::test]
fn board_chat_requests_route_sessions_and_cloud_replays_through_the_retained_shell(
    cx: &mut TestAppContext,
) {
    use subspace_chat::test_support::{AgentHandle, Command};
    use subspace_dashboard::{ChatRequest, CloudSession};
    let shell = Shell::open(cx, Placement::Bottom, 1_000., 900.);
    let (handle, mut commands) = AgentHandle::test_offline(shell._root.path().into(), Vec::new());
    let chat = shell.app.read_with(cx, |app, _| app.chat.clone());
    chat.update(cx, |chat, _| chat.set_test_runtime(handle));
    let board = shell.app.read_with(cx, |app, _| {
        app.dashboards[0].view().downcast::<Board>().ok().unwrap()
    });
    board.update(cx, |_, cx| {
        cx.emit(DashboardEvent::Chat(ChatRequest::OpenSession(
            "saved-fixture".into(),
        )))
    });
    shell.settle(cx);
    assert!(shell.app.read_with(cx, |app, _| app.chat_open));
    assert_eq!(shell.app.read_with(cx, |app, _| app.chat.clone()), chat);
    assert!(
        matches!(commands.try_recv(), Ok(Command::OpenChat { session: Some(session), .. }) if session == "saved-fixture")
    );
    shell
        .window
        .update(cx, |app, window, cx| {
            app.toggle_chat(&crate::app::ToggleChat, window, cx)
        })
        .unwrap();
    assert!(!shell.app.read_with(cx, |app, _| app.chat_open));
    assert!(
        commands.try_recv().is_err(),
        "hiding must not stop or replace the runtime"
    );
    board.update(cx, |_, cx| {
        cx.emit(DashboardEvent::Chat(ChatRequest::OpenCloudSession(
            CloudSession {
                task_id: "fixture-cloud-task".into(),
                title: "Cloud fixture".into(),
                url: "https://github.com/test/fixture/tasks/fixture-cloud-task".into(),
            },
        )))
    });
    shell.settle(cx);
    assert!(shell.app.read_with(cx, |app, _| app.chat_open));
    assert_eq!(shell.app.read_with(cx, |app, _| app.chat.clone()), chat);
    assert!(
        matches!(commands.try_recv(), Ok(Command::OpenRemote { task_id, .. }) if task_id == "fixture-cloud-task")
    );
    assert!(
        commands.try_recv().is_err(),
        "reopen must use the same runtime command queue"
    );
}

#[gpui_kit::test]
fn a_bottom_dock_yields_height_on_a_short_window_and_regains_it(cx: &mut TestAppContext) {
    let shell = Shell::open(cx, Placement::Bottom, 1_000., 900.);
    let (dock, _) = shell.extents(cx, BOTTOM_DOCK_ID);
    assert_extent(dock, PREFERRED_BOTTOM, "roomy window");

    shell.resize(cx, 1_000., 300.);
    let (dock, main) = shell.extents(cx, BOTTOM_DOCK_ID);
    assert_constrained(dock, main, PREFERRED_BOTTOM);
    assert_eq!(shell.preferred(cx).0, PREFERRED_BOTTOM);

    shell.resize(cx, 1_000., 900.);
    let (dock, _) = shell.extents(cx, BOTTOM_DOCK_ID);
    assert_extent(dock, PREFERRED_BOTTOM, "restored window");
}

fn assert_side_dock_yields_width_and_regains_it(cx: &mut TestAppContext, placement: Placement) {
    let shell = Shell::open(cx, placement, 2_000., 900.);
    let (dock, _) = shell.extents(cx, SIDE_DOCK_ID);
    assert_extent(dock, PREFERRED_SIDE, "roomy window");

    shell.resize(cx, 600., 900.);
    let (dock, main) = shell.extents(cx, SIDE_DOCK_ID);
    assert_constrained(dock, main, PREFERRED_SIDE);
    assert_eq!(shell.preferred(cx).1, PREFERRED_SIDE);

    shell.resize(cx, 2_000., 900.);
    let (dock, _) = shell.extents(cx, SIDE_DOCK_ID);
    assert_extent(dock, PREFERRED_SIDE, "restored window");
}

#[gpui_kit::test]
fn a_left_dock_yields_width_on_a_narrow_window_and_regains_it(cx: &mut TestAppContext) {
    assert_side_dock_yields_width_and_regains_it(cx, Placement::Left);
}

#[gpui_kit::test]
fn a_right_dock_yields_width_on_a_narrow_window_and_regains_it(cx: &mut TestAppContext) {
    assert_side_dock_yields_width_and_regains_it(cx, Placement::Right);
}

#[gpui_kit::test]
fn chat_leaves_the_board_visible_beside_either_side_dock_after_narrowing(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    for placement in [Placement::Left, Placement::Right] {
        let shell = Shell::open(cx, placement, 2_000., 900.);
        shell.app.update(cx, |app, cx| {
            // Render the existing panel without on_open, which starts a runtime.
            app.chat_open = true;
            cx.notify();
        });
        shell.resize(cx, 600., 900.);
        cx.update_window(shell.window.into(), |_, window, _| {
            let board = window.find(MAIN_ID).bounds();
            let chat = window.find("agent-chat").bounds();
            let dock = window.find(SIDE_DOCK_ID).bounds();
            assert!(board.size.width > px(0.), "Chat must not erase the board");
            assert!(board.right() <= chat.left() + px(TOLERANCE));
            assert!(board.size.width + chat.size.width + dock.size.width <= px(600. + TOLERANCE));
        })
        .unwrap();
        assert_eq!(shell.preferred(cx).1, PREFERRED_SIDE);
        shell.resize(cx, 2_000., 900.);
        cx.update_window(shell.window.into(), |_, window, _| {
            assert_eq!(window.find("agent-chat").bounds().size.width, px(420.));
            assert_eq!(
                window.find(SIDE_DOCK_ID).bounds().size.width,
                px(PREFERRED_SIDE)
            );
        })
        .unwrap();
        shell.app.read_with(cx, |_, cx| {
            assert_eq!(prefs::get(cx, "chat.width"), None);
            assert_eq!(prefs::get(cx, OPEN_KEY).as_deref(), Some("0"));
        });
    }
}
