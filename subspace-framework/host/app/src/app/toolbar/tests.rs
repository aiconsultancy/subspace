//! Production toolbar layout and interaction regressions.

use std::cell::Cell;

use gpui_kit::TestSupportExt as _;
use gpui_kit::component::{h_flex, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    TestAppContext, Window, WindowHandle, div, px,
};
use subspace_chat::OPEN_KEY;
use subspace_dashboard::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use subspace_dashboard::{Dashboard, DashboardEvent, prefs};

mod engine;
mod geometry;
mod height;
#[cfg(debug_assertions)]
mod inspector;
mod menu;
#[cfg(feature = "report-issue")]
mod report_issue;
mod sizing;
pub(in crate::app) mod status;
mod titlebar;
mod updates;

use geometry::{assert_toolbar_geometry, collect_toolbar_geometry};
use height::NoToolbarFixture;

use super::super::navigation::NAVIGATION_KEY;
use super::super::{SubspaceApp, registry};
use crate::theme;
use crate::toolbar_labels::ToolbarControl;

thread_local! {
    static FIXTURE_ACTIVATIONS: Cell<usize> = const { Cell::new(0) };
}

struct ToolbarFixture {
    focus: FocusHandle,
    toolbar: Entity<OverflowToolbar>,
}

impl EventEmitter<DashboardEvent> for ToolbarFixture {}

impl Focusable for ToolbarFixture {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ToolbarFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(Self::ID)
            .test_support()
            .track_focus(&self.focus)
            .size_full()
    }
}

impl Dashboard for ToolbarFixture {
    const ID: &'static str = "toolbar-fixture";
    const TITLE: &'static str = "A dashboard with a deliberately long title";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            toolbar: cx.new(|_| OverflowToolbar::new(fixture_units())),
        }
    }

    fn badge(&self, _: &App) -> Option<SharedString> {
        Some("999999".into())
    }

    fn render_toolbar(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui_kit::AnyElement> {
        self.toolbar
            .update(cx, |toolbar, cx| toolbar.set_units(fixture_units(), cx));
        Some(self.toolbar.clone().into_any_element())
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

fn fixture_units() -> Vec<OverflowUnit> {
    (0usize..8)
        .map(|index| {
            OverflowUnit::inline(
                format!("fixture-unit-{index}"),
                0.,
                move |_, _| fixture_control(index),
                vec![PopupEntry::new(move || {
                    PopupMenuItem::new(format!("Control {index}")).on_click(move |_, _, _| {
                        FIXTURE_ACTIVATIONS.with(|calls| calls.set(calls.get() + index + 1));
                    })
                })],
            )
        })
        .collect()
}

fn fixture_control(index: usize) -> gpui_kit::AnyElement {
    if index == 0 {
        let tabs = ["Critical & high", "Open", "Triaged"].map(|label| {
            subspace_dashboard::tabs::tab(subspace_dashboard::tabs::TabSpec::new(
                label,
                subspace_lucide::IconName::ShieldAlert,
            ))
        });
        return h_flex()
            .min_w_0()
            .child(subspace_dashboard::tabs::tab_bar("fixture-tabs").children(tabs))
            .into_any_element();
    }
    subspace_dashboard::action::action_button(
        ("fixture-control", index),
        subspace_dashboard::action::Intent::Neutral,
        None,
        format!("Control {index}"),
    )
    .w(px(100.))
    .into_any_element()
}

#[gpui_kit::test]
fn toolbar_geometry_is_stable_at_wide_narrow_and_vertical_sizes(cx: &mut TestAppContext) {
    for (mode, width, labels_on) in [
        ("horizontal", 1280., false),
        ("horizontal", 900., true),
        ("horizontal", 600., false),
        ("horizontal", 260., true),
        ("vertical", 900., false),
        ("vertical", 600., true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let window = open_fixture(cx, root.path(), mode, width, labels_on);
        settle(&window, cx);
        assert_toolbar_geometry(&window, cx, width, mode == "horizontal");
        if !labels_on && width >= 600. {
            cx.update_window(*window, |_, window, _| {
                assert!(
                    window.within("global-toolbar").try_find("help").is_some(),
                    "tabs must yield space before global controls overflow"
                );
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn narrow_global_and_board_popups_dispatch_real_callbacks(cx: &mut TestAppContext) {
    FIXTURE_ACTIVATIONS.with(|calls| calls.set(0));
    let root = tempfile::tempdir().unwrap();
    let window = open_fixture(cx, root.path(), "horizontal", 260., true);
    settle(&window, cx);

    cx.update_window(*window, |_, window, cx| {
        window
            .within("global-toolbar")
            .click("overflow-toolbar-button", cx);
        let help = popup_item(window, "Help");
        window.click(help, cx);
    })
    .unwrap();
    assert!(window.read_with(cx, |app, _| app.help_open).unwrap());

    cx.update_window(*window, |_, window, cx| {
        window
            .within("dashboard-toolbar")
            .click("overflow-toolbar-button", cx);
        let control = popup_item(window, "Control 1");
        window.click(control, cx);
    })
    .unwrap();
    assert_eq!(FIXTURE_ACTIVATIONS.with(Cell::get), 2);
}

#[gpui_kit::test]
fn tabs_popup_switches_dashboard_and_resize_restores_stable_layout(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open_fixture(cx, root.path(), "horizontal", 600., false);
    settle(&window, cx);
    cx.update_window(*window, |_, window, cx| {
        window
            .within("dashboard-tabs")
            .click("overflow-toolbar-button", cx);
        assert_eq!(
            window.within("popup-menu").find(1usize).label(),
            Some("A dashboard with a deliberately long title · 999999")
        );
        window.within("popup-menu").click(1usize, cx);
    })
    .unwrap();
    assert!(window.read_with(cx, |app, _| app.active > 0).unwrap());

    resize(&window, cx, 5000.);
    assert_toolbar_geometry(&window, cx, 5000., true);
    assert!(
        collect_toolbar_geometry(&window, cx).board_row.is_none(),
        "a fitting board group must return beside the tabs"
    );
    resize(&window, cx, 260.);
    assert_toolbar_geometry(&window, cx, 260., true);
    assert!(
        collect_toolbar_geometry(&window, cx).board_row.is_some(),
        "the complete group must move below the tabs"
    );
    resize(&window, cx, 5000.);
    assert_toolbar_geometry(&window, cx, 5000., true);
    assert!(collect_toolbar_geometry(&window, cx).board_row.is_none());
}

fn open_fixture(
    cx: &mut TestAppContext,
    root: &std::path::Path,
    mode: &str,
    width: f32,
    labels_on: bool,
) -> WindowHandle<SubspaceApp> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        theme::initialize(cx);
        crate::theme::preview_tests::nominal_controls(cx);
        crate::app::bind_keys(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, NAVIGATION_KEY, mode);
        prefs::set(cx, OPEN_KEY, "0");
        #[cfg(feature = "report-issue")]
        crate::app::report_issue::install_fixture_target(cx);
        for control in ToolbarControl::ALL {
            control.set_label(labels_on, cx);
        }
        cx.set_global(registry::BoardFactories(
            [
                ToolbarFixture::ID,
                "fixture-two",
                "fixture-three",
                "fixture-four",
                "fixture-five",
                "fixture-six",
                "fixture-seven",
                NoToolbarFixture::ID,
            ]
            .into_iter()
            .map(|id| {
                let mut registration = if id == NoToolbarFixture::ID {
                    registry::BoardRegistration::new::<NoToolbarFixture>()
                } else {
                    registry::BoardRegistration::new::<ToolbarFixture>()
                };
                registration.id = id;
                registration
            })
            .collect(),
        ));
    });
    cx.open_window(gpui_kit::size(px(width), px(600.)), |window, cx| {
        SubspaceApp::new(Vec::new(), window, cx)
    })
}

fn settle(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext) {
    for _ in 0..6 {
        cx.update_window((*window).into(), |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
}

fn resize(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext, width: f32) {
    cx.simulate_window_resize((*window).into(), gpui_kit::size(px(width), px(600.)));
    settle(window, cx);
}

fn popup_item(window: &Window, label: &str) -> usize {
    (0usize..16)
        .find(|index| {
            window
                .try_find(*index)
                .is_some_and(|item| item.label() == Some(label))
        })
        .unwrap_or_else(|| panic!("popup item {label:?} was not rendered"))
}
