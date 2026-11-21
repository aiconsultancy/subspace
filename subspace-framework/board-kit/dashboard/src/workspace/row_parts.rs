//! The parts a navigator row ([`super::NavRow`]) is drawn from: its fill,
//! its count, its context menu, its handlers and its focus.

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FocusHandle, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::row_order::{self, Step};
use crate::anchored::AnchoredContextMenuExt as _;
use crate::row_menu::{self, GroupedMenu};
use crate::tokens::Tokens;
use crate::{Activate, SelectNext, SelectPrevious};

/// A row's fill stops just short of the rows above and below it.
const FILL_GAP_PX: f32 = 1.;
/// A numeric count's monospace size at the default monospace setting.
const COUNT_MONO_PX: f32 = 11.;

pub(super) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// Builds a row's context menu as it opens.
pub(super) type MenuBuilder = Rc<dyn Fn(&mut Window, &mut App) -> GroupedMenu>;

/// Adds the row's context menu, when it has one. Opening it first runs
/// `select`, the click handler of a row that is not selected yet, which
/// selects the row.
pub(super) fn with_menu<E>(
    row: E,
    menu: Option<MenuBuilder>,
    select: Option<ClickHandler>,
) -> AnyElement
where
    E: InteractiveElement + ParentElement + Styled + IntoElement + 'static,
{
    let Some(commands) = menu else {
        return row.into_any_element();
    };
    row.anchored_context_menu(move |menu, window, cx| {
        if let Some(select) = &select {
            select(&ClickEvent::default(), window, cx);
        }
        let grouped = commands(window, cx);
        row_menu::build(menu, grouped, window, cx)
    })
    .into_any_element()
}

/// The fill behind a row's content: inset and rounded when the theme insets
/// rows, edge to edge otherwise. Hover shows only on a row that is not
/// selected. `focused` rings it in the accent while the keyboard is on the
/// row; the ring is drawn on the fill, so it never moves the row's text.
pub(super) fn fill(group: SharedString, selected: bool, focused: bool, tokens: &Tokens) -> Div {
    div()
        .absolute()
        .top(px(FILL_GAP_PX))
        .bottom(px(FILL_GAP_PX))
        .left(tokens.row_inset)
        .right(tokens.row_inset)
        .when(tokens.inset_rows(), |fill| {
            fill.rounded(tokens.control_radius)
        })
        .when(selected, |fill| fill.bg(tokens.select))
        .when(!selected, |fill| {
            fill.group_hover(group, |style| style.bg(tokens.hair))
        })
        .when(focused, |fill| fill.border_1().border_color(tokens.accent))
}

/// A row's count, in the monospace family when it is a number, so counts
/// line up; other quiet text, such as "Connected", keeps the caption face.
pub(super) fn count_mark(count: SharedString, mono: SharedString, tokens: &Tokens) -> Div {
    let number = !count.is_empty() && count.chars().all(|c| c.is_ascii_digit());
    div()
        .flex_shrink_0()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .when(number, |mark| {
            mark.font_family(mono)
                .text_size(tokens.mono_size(COUNT_MONO_PX))
        })
        .child(count)
}

/// The row's pointer and keyboard handlers. A click chooses the row but
/// leaves the keyboard focus where it was; Tab and the list keys bring it
/// here, and Enter chooses it as a click does. The list keys move only
/// between the navigator's rows and stop at the first and the last.
pub(super) fn with_handlers<E>(row: E, focus: FocusHandle, on_click: Option<ClickHandler>) -> E
where
    E: StatefulInteractiveElement + gpui_kit::prelude::FluentBuilder,
{
    let from = focus.clone();
    row.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .on_action(move |_: &SelectNext, window, cx| row_order::step(&from, Step::Next, window, cx))
        .on_action(move |_: &SelectPrevious, window, cx| {
            row_order::step(&focus, Step::Previous, window, cx)
        })
        .when_some(on_click, |row, on_click| {
            let activate = on_click.clone();
            row.on_click(move |event, window, cx| on_click(event, window, cx))
                .on_action(move |_: &Activate, window, cx| {
                    activate(&ClickEvent::default(), window, cx)
                })
        })
}

/// The row's own focus handle, kept across frames, in the tab order. A
/// `focusable` row also takes its place in the navigator's row order.
pub(super) fn row_focus(
    key: &SharedString,
    focusable: bool,
    window: &mut Window,
    cx: &mut App,
) -> FocusHandle {
    let focus = window
        .use_keyed_state(
            (ElementId::Name(key.clone()), "nav-row-focus"),
            cx,
            |_, cx| cx.focus_handle().tab_stop(true),
        )
        .read(cx)
        .clone();
    if focusable {
        row_order::push(&focus, window, cx);
    }
    focus
}
