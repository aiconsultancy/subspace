//! Height stability when switching between boards with and without controls.

use gpui_kit::TestSupportExt as _;
use gpui_kit::component::Theme;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, Render, Styled as _, TestAppContext, Window, div, px,
};
use subspace_dashboard::{Dashboard, DashboardEvent};

use super::{open_fixture, settle};

pub(super) struct NoToolbarFixture(FocusHandle);

impl EventEmitter<DashboardEvent> for NoToolbarFixture {}

impl Focusable for NoToolbarFixture {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.0.clone()
    }
}

#[gpui_kit::test]
fn vertical_row_controls_fit_and_stay_aligned_at_larger_font(cx: &mut TestAppContext) {
    for font_size in [20., 16.] {
        let root = tempfile::tempdir().unwrap();
        let window = open_fixture(cx, root.path(), "vertical", 1200., false);
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        cx.update_window(*window, |_, window, _| window.set_rem_size(px(font_size)))
            .unwrap();
        window
            .update(cx, |app, window, cx| app.switch_to(0, window, cx))
            .unwrap();
        settle(&window, cx);
        cx.update_window(*window, |_, window, _| {
            let row = window.find("app-toolbar").bounds();
            let logo = window.find("toolbar-logo").bounds();
            let global = window.within("global-toolbar").find("help").bounds();
            let board = window
                .within("dashboard-toolbar")
                .find(("fixture-control", 1usize))
                .bounds();
            for (name, bounds) in [("logo", logo), ("global", global), ("board", board)] {
                assert!(
                    (bounds.center().y - row.center().y).abs() <= px(1.),
                    "{name} off center at {font_size}: {bounds:?} row {row:?}"
                );
                assert!(
                    bounds.top() >= row.top() && bounds.bottom() <= row.bottom(),
                    "{name} outside row at {font_size}: {bounds:?} row {row:?}"
                );
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn narrow_vertical_row_keeps_board_overflow_at_larger_font(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open_fixture(cx, root.path(), "vertical", 400., false);
    cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(20.)));
    cx.update_window(*window, |_, window, _| window.set_rem_size(px(20.)))
        .unwrap();
    window
        .update(cx, |app, window, cx| app.switch_to(0, window, cx))
        .unwrap();
    settle(&window, cx);
    cx.update_window(*window, |_, window, cx| {
        assert!(window.try_find("board-toolbar-row").is_none());
        window
            .within("dashboard-toolbar")
            .click("overflow-toolbar-button", cx);
        assert!(window.try_find("popup-menu").is_some());
    })
    .unwrap();
}

impl Render for NoToolbarFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(Self::ID)
            .test_support()
            .track_focus(&self.0)
            .size_full()
    }
}

impl Dashboard for NoToolbarFixture {
    const ID: &'static str = "no-toolbar-fixture";
    const TITLE: &'static str = "No toolbar";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self(cx.focus_handle())
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

#[gpui_kit::test]
fn vertical_toolbar_height_does_not_change_when_switching_boards(cx: &mut TestAppContext) {
    for font_size in [16., 20.] {
        let root = tempfile::tempdir().unwrap();
        let horizontal = open_fixture(cx, root.path(), "horizontal", 5000., false);
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        cx.update_window(*horizontal, |_, window, _| {
            window.set_rem_size(px(font_size))
        })
        .unwrap();
        settle(&horizontal, cx);
        horizontal
            .update(cx, |app, window, cx| app.switch_to(7, window, cx))
            .unwrap();
        settle(&horizontal, cx);
        let expected = cx
            .update_window(*horizontal, |_, window, _| {
                window.find("app-toolbar").bounds().size.height
            })
            .unwrap();

        let root = tempfile::tempdir().unwrap();
        let vertical = open_fixture(cx, root.path(), "vertical", 900., false);
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        cx.update_window(*vertical, |_, window, _| window.set_rem_size(px(font_size)))
            .unwrap();
        vertical
            .update(cx, |app, window, cx| app.switch_to(0, window, cx))
            .unwrap();
        settle(&vertical, cx);
        let before = cx
            .update_window(*vertical, |_, window, _| {
                (
                    window.find("app-toolbar").bounds().size.height,
                    window.find("toolbar-fixture").bounds().top(),
                )
            })
            .unwrap();
        vertical
            .update(cx, |app, window, cx| app.switch_to(7, window, cx))
            .unwrap();
        settle(&vertical, cx);
        let after = cx
            .update_window(*vertical, |_, window, _| {
                (
                    window.find("app-toolbar").bounds().size.height,
                    window.find("no-toolbar-fixture").bounds().top(),
                )
            })
            .unwrap();
        assert_eq!(
            before.0, expected,
            "vertical toolbar must match the horizontal first row"
        );
        assert_eq!(
            after.0, expected,
            "toolbar-less board must reserve the same height"
        );
        assert_eq!(before.1, after.1, "board content must not jump");
    }
}
