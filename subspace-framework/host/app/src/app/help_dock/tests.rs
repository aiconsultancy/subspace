use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, TestAppContext, TestSupportExt as _,
    Window, WindowBounds, WindowOptions, div, px, size,
};
use subspace_dashboard::prefs;

use super::super::super::SubspaceApp;
use super::super::state::{self, HelpDockState, Side};
use super::HelpDock;

struct Fixture {
    dock: Entity<HelpDock>,
    _owner: Entity<SubspaceApp>,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.dock.clone())
    }
}

fn open_fixture(
    stored: Option<&str>,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<Fixture>, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let (window, fixture) = cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
        cx.set_global(crate::registry::BoardFactories(Vec::new()));
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, subspace_chat::OPEN_KEY, "0");
        if let Some(stored) = stored {
            prefs::set(cx, state::KEY, stored);
        }
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1_200.), px(800.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let owner = cx.new(|cx| SubspaceApp::new(Vec::new(), window, cx));
                let dock = HelpDock::create(
                    &owner,
                    |_, _, _| {
                        div()
                            .id("original-workspace")
                            .test_support()
                            .size_full()
                            .into_any_element()
                    },
                    window,
                    cx,
                );
                cx.new(|_| Fixture {
                    dock,
                    _owner: owner,
                })
            },
        )
        .unwrap()
    });
    cx.run_until_parked();
    (window, fixture, root)
}

#[gpui_kit::test]
fn native_zoom_retains_close_and_close_collapses_without_removing_help(cx: &mut TestAppContext) {
    let (window, fixture, _root) = open_fixture(None, cx);
    let dock = fixture.read_with(cx, |fixture, _| fixture.dock.clone());
    cx.update_window(window, |_, window, cx| {
        dock.update(cx, |dock, cx| dock.toggle(window, cx));
        window.render_frame(cx);
        assert!(window.find("original-workspace").bounds().size.width > px(0.));
        window.click("zoom-in", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(dock.read(cx).area.read(cx).is_zoomed());
        assert!(window.find("help-dock-close").visible());
        window.click("help-dock-close", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let host = dock.read(cx);
        assert!(!host.is_open());
        assert!(!host.area.read(cx).is_zoomed());
        assert!(
            host.area
                .read(cx)
                .has_dock(gpui_kit::component::dock::DockPlacement::Right)
        );
        let saved = HelpDockState::decode(&prefs::get(cx, state::KEY).unwrap()).unwrap();
        assert!(!saved.open);
        assert!(window.find("original-workspace").bounds().size.width > px(1_000.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn side_changes_and_native_resizing_save_only_the_help_preference(cx: &mut TestAppContext) {
    let (window, fixture, _root) = open_fixture(None, cx);
    let dock = fixture.read_with(cx, |fixture, _| fixture.dock.clone());
    cx.update_window(window, |_, window, cx| {
        prefs::set(cx, "terminal.dock-layout", "test-only board layout");
        dock.update(cx, |host, cx| {
            host.toggle(window, cx);
            host.move_side(Side::Left, window, cx);
            host.area.update(cx, |area, cx| {
                area.set_dock_size(
                    gpui_kit::component::dock::DockPlacement::Left,
                    px(420.),
                    window,
                    cx,
                );
            });
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let saved = HelpDockState::decode(&prefs::get(cx, state::KEY).unwrap()).unwrap();
        assert_eq!(saved.side, Side::Left);
        assert_eq!(saved.width, 420.);
        assert!(saved.open);
        assert_eq!(
            prefs::get(cx, "terminal.dock-layout").as_deref(),
            Some("test-only board layout")
        );
    });
}

#[gpui_kit::test]
fn future_help_preferences_survive_session_layout_changes_and_other_saves(cx: &mut TestAppContext) {
    const FUTURE: &str = r#"{"version":2,"future":"test-only value"}"#;
    let (window, fixture, root) = open_fixture(Some(FUTURE), cx);
    let dock = fixture.read_with(cx, |fixture, _| fixture.dock.clone());
    cx.update_window(window, |_, window, cx| {
        dock.update(cx, |host, cx| {
            host.toggle(window, cx);
            host.move_side(Side::Left, window, cx);
            host.close(window, cx);
        });
        prefs::set(cx, "unrelated", "changed");
        prefs::save(cx).unwrap();
        assert_eq!(prefs::get(cx, state::KEY).as_deref(), Some(FUTURE));
    })
    .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.path().join("state.json")).unwrap()).unwrap();
    assert_eq!(saved[state::KEY], FUTURE);
    assert_eq!(saved["unrelated"], "changed");
}

#[gpui_kit::test]
fn native_minimum_clamp_does_not_replace_the_preferred_width(cx: &mut TestAppContext) {
    let stored = HelpDockState {
        side: Side::Right,
        width: 500.,
        open: true,
    }
    .encode()
    .unwrap();
    let (window, fixture, _root) = open_fixture(Some(&stored), cx);
    let dock = fixture.read_with(cx, |fixture, _| fixture.dock.clone());
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.simulate_window_resize(window, size(px(100.), px(800.)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        dock.update(cx, |host, cx| host.clamp(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        dock.update(cx, |host, cx| {
            host.layout_changed(window, cx);
            assert_eq!(host.state.width, 500.);
            assert_eq!(
                host.applied_width,
                f32::from(gpui_kit::base::PANEL_MIN_SIZE)
            );
            assert_eq!(
                host.area
                    .read(cx)
                    .dock_size(gpui_kit::component::dock::DockPlacement::Right),
                Some(gpui_kit::base::PANEL_MIN_SIZE)
            );
        });
        assert_eq!(prefs::get(cx, state::KEY).as_deref(), Some(stored.as_str()));
    })
    .unwrap();
}

#[gpui_kit::test]
fn native_menu_moves_help_to_both_edges_without_replacing_the_workspace(cx: &mut TestAppContext) {
    let (window, fixture, _root) = open_fixture(None, cx);
    let dock = fixture.read_with(cx, |fixture, _| fixture.dock.clone());
    cx.update_window(window, |_, window, cx| {
        dock.update(cx, |host, cx| host.toggle(window, cx));
        window.render_frame(cx);
        window.click("menu", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(dock.read(cx).state.side, Side::Left);
        assert!(
            window.find("help-dock-scroll").bounds().right()
                <= window.find("original-workspace").bounds().left()
        );
        // The keyboard skips the disabled "Move Help left".
        window.click("menu", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(dock.read(cx).state.side, Side::Right);
        assert!(
            window.find("help-dock-scroll").bounds().left()
                >= window.find("original-workspace").bounds().right()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_new_host_restores_saved_side_width_and_open_state_but_not_zoom(cx: &mut TestAppContext) {
    let (window, fixture, root) = open_fixture(None, cx);
    cx.update_window(window, |_, window, cx| {
        let host = fixture.read(cx).dock.clone();
        host.update(cx, |host, cx| {
            host.toggle(window, cx);
            host.move_side(Side::Left, window, cx);
            host.area.update(cx, |area, cx| {
                area.set_dock_size(
                    gpui_kit::component::dock::DockPlacement::Left,
                    px(420.),
                    window,
                    cx,
                )
            });
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("zoom-in", cx);
        prefs::save(cx).unwrap();
        prefs::init_file(Some(root.path().join("state.json")), cx);
        fixture.update(cx, |fixture, cx| {
            fixture.dock = HelpDock::create(
                &fixture._owner,
                |_, _, _| div().size_full().into_any_element(),
                window,
                cx,
            );
            cx.notify();
        });
        let host = fixture.read(cx).dock.read(cx);
        assert_eq!(
            host.state,
            HelpDockState {
                side: Side::Left,
                width: 420.,
                open: true
            }
        );
        assert!(!host.area.read(cx).is_zoomed());
    })
    .unwrap();
}

#[path = "resize_tests.rs"]
mod resize;
