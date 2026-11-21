//! The parts a navigator row ([`super::NavRow`]) is drawn from: its fill,
//! its count and its context menu.

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, Window, div, px,
};

use crate::anchored::AnchoredContextMenuExt as _;
use crate::row_menu::{self, GroupedMenu};
use crate::tokens::Tokens;

/// A row's fill stops just short of the rows above and below it.
const FILL_GAP_PX: f32 = 1.;
/// A numeric count's monospace size at the default monospace setting.
const COUNT_MONO_PX: f32 = 11.;

pub(super) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// Builds a row's context menu as it opens.
pub(super) type MenuBuilder = Rc<dyn Fn(&mut Window, &mut App) -> GroupedMenu>;

/// Adds the row's context menu, when it has one. Opening it first runs the
/// row's click handler, which selects the row.
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
/// selected.
pub(super) fn fill(group: SharedString, selected: bool, tokens: &Tokens) -> Div {
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
