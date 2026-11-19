//! The two kinds of line in a list: a group header and an item row.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, ClickEvent, Context, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    TestSupportExt as _, div, px,
};

use super::super::{
    FOLD_ARROW_WIDTH_PX, Glyph, Group, GroupIndicator, GroupNote, ListDashboard, ROW_FILL_GAP_PX,
    ROW_ID_MIN_WIDTH_PX, ROW_PADDING_LEFT_PX, Row, WRAPPED_ROW_PADDING_Y_PX, activate, line_menu,
    update,
};
use crate::anchored::AnchoredContextMenuExt as _;
use crate::feedback;
use crate::tokens::Tokens;
use crate::truncated;
use crate::typography;

/// Shared row frame: height, padding, separator and selection fill. With a
/// row inset, hover and selection are a rounded fill inside the row and rows
/// have no separators.
fn row_frame(key: &SharedString, selected: bool, wrap: bool, tokens: &Tokens) -> Stateful<Div> {
    let capsule = tokens.inset_rows();
    let group = SharedString::from(format!("list-row:{key}"));
    h_flex()
        .id(ElementId::Name(key.clone()))
        .group(group.clone())
        .relative()
        .w_full()
        .flex_shrink_0()
        .map(|row| {
            if wrap {
                row.min_h(tokens.row_height)
                    .py(px(WRAPPED_ROW_PADDING_Y_PX))
                    .items_start()
            } else {
                row.h(tokens.row_height).whitespace_nowrap()
            }
        })
        .gap_2()
        .px(tokens.pad_x + tokens.row_inset)
        .text_size(tokens.row_font)
        .map(|row| {
            if capsule {
                row.child(row_fill(group, selected, tokens))
            } else {
                row.border_b_1()
                    .border_color(tokens.hair)
                    .when(selected, |row| row.bg(tokens.select))
            }
        })
}

/// The inset rounded fill behind a capsule row's content.
fn row_fill(group: SharedString, selected: bool, tokens: &Tokens) -> Div {
    div()
        .absolute()
        .top(px(ROW_FILL_GAP_PX))
        .bottom(px(ROW_FILL_GAP_PX))
        .left(tokens.row_inset)
        .right(tokens.row_inset)
        .rounded(tokens.control_radius)
        .when(selected, |fill| fill.bg(tokens.select))
        .when(!selected, |fill| {
            fill.group_hover(group, |style| style.bg(tokens.hair))
        })
}

/// A title: wrapped lines, or one line that offers the whole title in a
/// tooltip when the row cuts it.
fn title(id: impl Into<ElementId>, text: SharedString, wrap: bool) -> truncated::Shortened {
    truncated::text(id, text).wrap(wrap).flex_1()
}

fn glyph(glyph: &Glyph, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .text_color(tokens.glyph(glyph))
        .child(glyph.text.clone())
}

/// A group header's trailing status marks, right of its name and count.
/// Presentation only: no click behaviour, each with an accessible label and
/// a hover tooltip.
fn group_indicators(
    group_key: &SharedString,
    indicators: Vec<GroupIndicator>,
    tokens: &Tokens,
) -> Option<Div> {
    if indicators.is_empty() {
        return None;
    }
    Some(
        h_flex()
            .flex_shrink_0()
            .gap_1()
            .children(indicators.into_iter().enumerate().map(|(index, mark)| {
                let color = tokens.tone(mark.tone);
                let id =
                    ElementId::Name(format!("list-group-indicator:{group_key}:{index}").into());
                let tooltip = mark.tooltip.clone();
                div()
                    .id(id)
                    .flex_shrink_0()
                    .role(Role::Image)
                    .aria_label(mark.label)
                    .child(Icon::from(mark.icon).xsmall().text_color(color))
                    .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            })),
    )
}

/// A group header's frame: the shared row, then either the theme's filled
/// band and top line, or, for a comfortable list, a little space above and
/// only the row's hairline.
fn group_frame<D: ListDashboard>(
    this: &D,
    key: &SharedString,
    selected: bool,
    wrap: bool,
    tokens: &Tokens,
) -> Stateful<Div> {
    let comfortable = this.comfortable_rows();
    let banded = !tokens.inset_rows() && !comfortable;
    row_frame(key, selected, wrap, tokens)
        .pl(px(ROW_PADDING_LEFT_PX) + tokens.row_inset)
        .when_some(this.list_side_inset(), |row, inset| row.px(inset))
        .when(comfortable, |row| row.mt_1())
        .when(!selected && banded, |row| row.bg(tokens.pane))
        .when(banded, |row| row.border_t_1().border_color(tokens.line))
}

pub(super) fn group_row<D: ListDashboard>(
    this: &D,
    group: &Group,
    folded: bool,
    tokens: &Tokens,
    cx: &mut Context<D>,
) -> AnyElement {
    let list = this.list();
    let wrap = crate::wrap(cx);
    let selected = list.selected() == Some(&group.key);
    let key = group.key.clone();
    let facts = this.group_menu(group);
    let indicators = group_indicators(&group.key, this.group_indicators(group, cx), tokens);
    let capsule = tokens.inset_rows();
    let header = group_frame(this, &group.key, selected, wrap, tokens)
        .child(
            div()
                .w(px(FOLD_ARROW_WIDTH_PX))
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(if folded { "▸" } else { "▾" }),
        )
        .child(
            this.group_leading(group, cx)
                .unwrap_or_else(|| glyph(&group.icon, tokens).into_any_element()),
        )
        .child(
            typography::group_heading(
                title(
                    SharedString::from(format!("list-group-title:{}", group.key)),
                    group.name.clone(),
                    wrap,
                )
                .when(this.group_count_after_name(), |name| name.flex_initial()),
                cx,
            )
            .text_color(if capsule {
                tokens.muted
            } else {
                tokens.fg_strong
            }),
        )
        .when_some(group.count.clone(), |row, count| {
            row.child(group_count(&group.key, count, tokens))
        })
        .when_some(indicators, |row, indicators| row.child(indicators))
        .on_click(cx.listener(move |this: &mut D, _, _, cx| {
            update(this, cx, |list| list.select(key.clone()));
            activate(this, cx);
        }));
    match facts {
        Some(facts) => {
            let menu = line_menu(group.key.clone(), facts, cx);
            header
                .anchored_context_menu(move |popup, window, cx| menu(popup, window, cx))
                .into_any_element()
        }
        None => header.into_any_element(),
    }
}

fn group_count(key: &str, count: SharedString, tokens: &Tokens) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("list-group-count:{key}")))
        .test_support()
        .text_xs()
        .text_color(tokens.muted)
        .child(count)
}

pub(super) fn item_row<D: ListDashboard>(
    this: &D,
    row: &Row,
    tokens: &Tokens,
    cx: &mut Context<D>,
) -> AnyElement {
    let list = this.list();
    let wrap = crate::wrap(cx);
    let selected = list.selected() == Some(&row.key);
    let key = row.key.clone();
    let menu = line_menu(row.key.clone(), this.row_menu(row), cx);
    let single_click = this.activates_on_click(row);
    let title_color = if row.done && !selected {
        tokens.muted
    } else {
        tokens.fg
    };
    row_frame(&row.key, selected, wrap, tokens)
        .role(Role::ListItem)
        .aria_label(this.row_accessible_label(row, cx))
        .pl(px(ROW_PADDING_LEFT_PX)
            + tokens.row_inset
            + tokens.indent * (1.0 + f32::from(row.indent)))
        .when_some(this.list_side_inset(), |line, inset| {
            line.pr(inset)
                .pl(inset + tokens.indent * (1.0 + f32::from(row.indent)))
        })
        .child(
            this.row_leading(row, cx)
                .unwrap_or_else(|| glyph(&row.icon, tokens).into_any_element()),
        )
        .when_some(row.id.clone(), |el, id| {
            el.child(
                div()
                    .flex_shrink_0()
                    .min_w(px(ROW_ID_MIN_WIDTH_PX))
                    .text_xs()
                    .text_color(tokens.muted)
                    .child(id),
            )
        })
        .child(
            title(
                SharedString::from(format!("list-row-title:{}", row.key)),
                row.title.clone(),
                wrap,
            )
            .text_color(title_color)
            .when(this.row_emphasis(row, cx), |title| {
                title.font_weight(FontWeight::BOLD)
            }),
        )
        .map(|el| match this.row_trailing(row, cx) {
            Some(lanes) => el.child(lanes),
            None => el.children(row.trailing.iter().map(|g| glyph(g, tokens))),
        })
        .on_click(cx.listener(move |this: &mut D, event: &ClickEvent, _, cx| {
            update(this, cx, |list| list.select(key.clone()));
            if single_click || event.click_count() > 1 {
                activate(this, cx);
            }
        }))
        .anchored_context_menu(move |popup, window, cx| menu(popup, window, cx))
        .into_any_element()
}

/// A group's note: a quiet line indented like the group's rows, which the
/// selection skips. Loading uses the shared loading line, Empty the shared
/// empty line. Its text stays selectable for copying.
fn note_line<D: ListDashboard>(
    this: &D,
    group: &Group,
    note: GroupNote,
    tokens: &Tokens,
) -> impl IntoElement {
    let text_id = SharedString::from(format!("list-group-note:{}", group.key));
    let label = note.label(&group.name);
    let text = match note {
        GroupNote::Loading(message) => feedback::loading_line(tokens, text_id, message),
        GroupNote::Empty(message) => feedback::empty_line(tokens, text_id, message),
    };
    let left = px(ROW_PADDING_LEFT_PX) + tokens.row_inset + tokens.indent;
    h_flex()
        .id(SharedString::from(format!(
            "list-group-note-line:{}",
            group.key
        )))
        .test_support()
        .role(Role::ListItem)
        .aria_label(label)
        .w_full()
        .flex_shrink_0()
        .h(tokens.row_height)
        .px(tokens.pad_x + tokens.row_inset)
        .pl(left)
        .when_some(this.list_side_inset(), |line, inset| {
            line.pr(inset).pl(inset + tokens.indent)
        })
        .when(!tokens.inset_rows(), |line| {
            line.border_b_1().border_color(tokens.hair)
        })
        .child(text)
}

/// A group's last line with its note under it, drawn as one line.
pub(super) fn with_note<D: ListDashboard>(
    this: &D,
    last: AnyElement,
    group: &Group,
    note: GroupNote,
    tokens: &Tokens,
) -> AnyElement {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .child(last)
        .child(note_line(this, group, note, tokens))
        .into_any_element()
}
