//! Render, click and keyboard tests of the Space in a window, reading the
//! fictional fixture decisions. The pure modules keep their own tests.

mod account;
mod issues;
mod list;
mod page;

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, Entity, Subscription, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions, px, size,
};
use subspace_framework::dashboard::{
    BoardNavigation, Dashboard as _, DashboardEvent, ListDashboard as _, list as shared_list,
};

use crate::load::Reader;
use crate::{Decisions, fixtures, lanes, listing};

/// The mock-up's size.
const WIDTH: f32 = 1440.;
const HEIGHT: f32 = 900.;

/// The Space in a 1440 × 900 window, shown, with its first read landed.
pub(crate) fn mount(cx: &mut TestAppContext) -> (Entity<Decisions>, &mut VisualTestContext) {
    mount_with(fixtures::reader(), cx)
}

/// The Space reading with `reader`.
pub(crate) fn mount_with(
    reader: Reader,
    cx: &mut TestAppContext,
) -> (Entity<Decisions>, &mut VisualTestContext) {
    bind(cx);
    let (window, board) = cx.update(|cx| {
        gpui_kit::open_window(window_options(), cx, |window, cx| {
            cx.new(|cx| Decisions::new(window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    board.update(cx, move |board, _| board.reader = reader);
    cx.update(|window, cx| {
        window.activate_window();
        board.update(cx, |board, cx| {
            board.focus.focus(window, cx);
            board.activate(window, cx);
        });
        window.render_frame(cx);
    });
    render(cx);
    (board, cx)
}

/// GPUI Kit, the preferences, and the shared keys and the Space's.
fn bind(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        subspace_framework::dashboard::prefs::init_file(None, cx);
        subspace_framework::dashboard::bind_keys(cx);
        Decisions::bind_keys(cx);
    });
}

/// A window the mock-up's size.
fn window_options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            Default::default(),
            size(px(WIDTH), px(HEIGHT)),
        ))),
        ..Default::default()
    }
}

pub(crate) fn render(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

pub(crate) fn has(cx: &mut VisualTestContext, id: impl Into<gpui_kit::ElementId>) -> bool {
    let id = id.into();
    cx.update(|window, _| window.try_find(id).is_some())
}

/// The accessible name of the element `id`.
pub(crate) fn label(
    cx: &mut VisualTestContext,
    id: impl Into<gpui_kit::ElementId>,
) -> Option<String> {
    let id = id.into();
    cx.update(|window, _| {
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
}

/// Whether an element is named `text` for assistive technology.
pub(crate) fn labelled(cx: &mut VisualTestContext, text: &str) -> bool {
    cx.update(|window, _| window.try_find_by_label(text).is_some())
}

pub(crate) fn press(cx: &mut VisualTestContext, key: &str) {
    cx.update(|window, cx| window.press(key, cx));
    render(cx);
}

pub(crate) fn click(cx: &mut VisualTestContext, id: impl Into<gpui_kit::ElementId>) {
    let id = id.into();
    cx.update(|window, cx| window.click(id, cx));
    render(cx);
}

/// Clicks decision `id`'s row, at its lanes.
pub(crate) fn click_row(cx: &mut VisualTestContext, id: &str) {
    click(cx, lanes::lanes_id(&listing::row_key(id)));
}

pub(crate) fn double_click_row(cx: &mut VisualTestContext, id: &str) {
    let lanes = lanes::lanes_id(&listing::row_key(id));
    cx.update(|window, cx| window.double_click(lanes, cx));
    render(cx);
}

/// Selects decision `id`'s row as the list keys do.
pub(crate) fn select(board: &Entity<Decisions>, id: &str, cx: &mut VisualTestContext) {
    let key = listing::row_key(id);
    board.update(cx, |board, cx| {
        shared_list::update(board, cx, |list| list.select(key))
    });
    render(cx);
}

pub(crate) fn selected(board: &Entity<Decisions>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| board.read(cx).list().selected().map(ToString::to_string))
}

/// The list keys of the rows drawn, in order.
pub(crate) fn rows(board: &Entity<Decisions>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        let list = board.read(cx).list();
        list.visible_keys()
            .into_iter()
            .filter(|key| listing::decision_id(key).is_some())
            .map(|key| key.to_string())
            .collect()
    })
}

/// What the Space asked the shell for.
#[derive(Default)]
pub(crate) struct Events {
    pub(crate) navigations: Vec<BoardNavigation>,
    pub(crate) notices: Vec<String>,
}

pub(crate) fn observe(
    board: &Entity<Decisions>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Events>>, Subscription) {
    let events = Rc::new(RefCell::new(Events::default()));
    let received = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(board, move |_, event, _| match event {
            DashboardEvent::Navigate(navigation) => {
                received.borrow_mut().navigations.push(navigation.clone());
            }
            DashboardEvent::Toast(toast) => {
                received.borrow_mut().notices.push(toast.text.to_string());
            }
            _ => {}
        })
    });
    (events, subscription)
}
