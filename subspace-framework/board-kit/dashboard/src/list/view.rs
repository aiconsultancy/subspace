//! How a list dashboard draws itself: the lines of the scroll container
//! and the group header and item rows.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, InteractiveElement, IntoElement, KeyContext,
    OperatingSystem, ParentElement, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::{ListDashboard, ListState, activate, update};
use crate::tokens::Tokens;
use crate::{
    Activate, CloseDetail, DASHBOARD_CONTEXT, SelectFirst, SelectLast, SelectNext, SelectPrevious,
};

mod heading;
pub use heading::header;

/// The list's own keys: moving the selection, Enter and closing the panel.
fn with_list_keys<D: ListDashboard>(list: Stateful<Div>, cx: &mut Context<D>) -> Stateful<Div> {
    list.on_action(cx.listener(|this: &mut D, _: &SelectNext, _, cx| {
        update(this, cx, |list| list.step(1));
    }))
    .on_action(cx.listener(|this: &mut D, _: &SelectPrevious, _, cx| {
        update(this, cx, |list| list.step(-1));
    }))
    .on_action(cx.listener(|this: &mut D, _: &SelectFirst, _, cx| {
        update(this, cx, |list| list.select_end(false));
    }))
    .on_action(cx.listener(|this: &mut D, _: &SelectLast, _, cx| {
        update(this, cx, |list| list.select_end(true));
    }))
    .on_action(cx.listener(|this: &mut D, _: &Activate, _, cx| activate(this, cx)))
    .on_action(cx.listener(|this: &mut D, _: &CloseDetail, _, cx| {
        update(this, cx, ListState::close_detail);
    }))
}

/// The dashboard's key context: the shared list keys and its own.
fn key_context<D: ListDashboard>(os: OperatingSystem) -> KeyContext {
    let mut context = KeyContext::new_with_defaults(os);
    context.add(DASHBOARD_CONTEXT);
    context.add(D::ID);
    context
}

/// Brings the selected line into view when one is pending. `first_row` is
/// the number of lines drawn before the first group line.
fn scroll_to_selected(list: &ListState, first_row: usize) {
    if let Some(line) = list.take_scroll_request() {
        list.scroll_handle().scroll_to_item(first_row + line);
    }
}

/// Draw `this` as the list and, when a row is open, the detail panel. Call
/// it from the dashboard's `Render::render`, and add `on_action` handlers
/// for the dashboard's own actions to the result.
// qual:api - every list dashboard draws itself with it.
pub fn render<D: ListDashboard>(
    this: &D,
    window: &mut Window,
    cx: &mut Context<D>,
) -> Stateful<Div> {
    framed(this, None, window, cx)
}

/// As [`render`], with `head` above the list and beside the detail panel.
/// Pass a workspace pane's tab strip and toolbar (`workspace::pane_head`):
/// the panel then keeps the frame's full height, so its header lines up
/// with the tab strip as a workspace dock's does.
// qual:api - list dashboards in a workspace draw themselves with it.
pub fn render_under<D: ListDashboard>(
    this: &D,
    head: impl IntoElement,
    window: &mut Window,
    cx: &mut Context<D>,
) -> Stateful<Div> {
    framed(this, Some(head.into_any_element()), window, cx)
}

fn framed<D: ListDashboard>(
    this: &D,
    head: Option<AnyElement>,
    window: &mut Window,
    cx: &mut Context<D>,
) -> Stateful<Div> {
    let tokens = Tokens::current(cx);
    let retry = cx.listener(|this: &mut D, _: &ClickEvent, window, cx| this.reload(window, cx));
    let (lines, first_row) = lines(this, &tokens, retry, cx);
    let list = this.list();
    scroll_to_selected(list, first_row);
    let panel = list
        .open_row()
        .and_then(|row| detail_panel(this, row, window, cx));
    let column = list_column(this, head, lines);

    let frame = h_flex()
        .id(D::ID)
        .key_context(key_context::<D>(cx.platform_info().os))
        .track_focus(&this.focus_handle(cx));
    with_list_keys(frame, cx)
        .size_full()
        .min_h_0()
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(column)
        .children(panel.map(|panel| div().min_w_0().h_full().child(panel)))
}

/// The scrolling list, alone or under `head` in one column.
fn list_column<D: ListDashboard>(
    this: &D,
    head: Option<AnyElement>,
    lines: Vec<AnyElement>,
) -> AnyElement {
    let list = v_flex()
        .id("list")
        .flex_1()
        .overflow_y_scroll()
        .track_scroll(this.list().scroll_handle())
        .children(lines);
    let min_width = px(this.min_list_width());
    match head {
        None => list.min_w(min_width).h_full().into_any_element(),
        Some(head) => v_flex()
            .flex_1()
            .min_w(min_width)
            .h_full()
            .min_h_0()
            .child(head)
            .child(list.w_full().min_h_0())
            .into_any_element(),
    }
}
mod captions;
mod rows;

mod scroll_lines;
use scroll_lines::lines;

mod panel_view;

use panel_view::detail_panel;
pub use panel_view::render_body;
