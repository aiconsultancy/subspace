use std::{cell::Cell, rc::Rc};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Focusable as _, TestAppContext, VisualTestContext, px, size};
use subspace_core::log::{self, Logger};

use super::toolbar::settle;
use crate::log_view::{LogView, LogViewEvent};
use crate::measure::text_width;

fn closing_view(
    cx: &mut TestAppContext,
) -> (Entity<LogView>, &mut VisualTestContext, Rc<Cell<usize>>) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(LogView::new);
    let closed = Rc::new(Cell::new(0));
    let observed = closed.clone();
    cx.update(|window, cx| {
        window
            .subscribe(&view, cx, move |_, _: &LogViewEvent, _, _| {
                observed.set(observed.get() + 1)
            })
            .detach();
    });
    (view, cx, closed)
}

#[gpui_kit::test]
fn escape_from_the_filter_input_closes_the_view(cx: &mut TestAppContext) {
    let (view, cx, closed) = closing_view(cx);
    cx.update(|window, cx| {
        crate::bind_keys(cx);
        let input = view.read(cx).input.focus_handle(cx);
        input.focus(window, cx);
        window.render_frame(cx);
        assert!(input.is_focused(window));
        window.press("escape", cx);
    });
    cx.run_until_parked();
    assert_eq!(closed.get(), 1);
}

#[gpui_kit::test]
fn the_header_close_is_visible_beside_a_collapsed_toolbar(cx: &mut TestAppContext) {
    let (_, cx, closed) = closing_view(cx);
    cx.simulate_resize(size(px(360.), px(400.)));
    settle(cx);
    cx.update(|window, cx| {
        let header = window.find("surface-header").bounds();
        let toolbar = window.find("log-toolbar").bounds();
        let close = window.find("surface-close");
        assert_eq!(close.label(), Some("Close logs"));
        assert!(window.try_find("overflow-toolbar-button").is_some());
        assert!(window.try_find("log-close").is_none());
        assert!(header.bottom() <= toolbar.top());
        assert!(close.bounds().right() <= header.right());
        window.click("surface-close", cx);
    });
    cx.run_until_parked();
    assert_eq!(closed.get(), 1);
}

#[gpui_kit::test]
fn the_level_lane_fits_error_on_one_line(cx: &mut TestAppContext) {
    let target = "log-level-lane-fixture";
    Logger::new(target).error("lane fixture");
    let record = log::recent()
        .into_iter()
        .rev()
        .find(|record| record.target == target)
        .unwrap();
    let (_, cx, _) = closing_view(cx);
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.update(|window, cx| {
        window.render_frame(cx);
        let level = window.find(("log-level", record.seq)).bounds();
        let message = window.find(("log-message", record.seq)).bounds();
        let tokens = crate::Tokens::current(cx);
        let mono = cx.theme().mono_font_family.clone();
        let error = text_width(window, "ERROR", mono, tokens.mono_font);
        assert!(
            level.size.width >= error,
            "{level:?} narrower than {error:?}"
        );
        assert_eq!(level.size.height, message.size.height, "ERROR wrapped");
        assert!(level.right() < message.left());
    });
}
