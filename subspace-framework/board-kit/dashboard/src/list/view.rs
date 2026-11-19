//! How a list dashboard draws itself: the lines of the scroll container
//! and the group header and item rows.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Div, InteractiveElement, IntoElement, KeyContext,
    OperatingSystem, ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use super::{
    COMFORTABLE_ROW_SCALE, Group, LIST_TAIL_HEIGHT_PX, ListDashboard, ListState, LoadStatus,
    activate, update,
};
use crate::feedback::{self, Words};
use crate::tokens::Tokens;
use crate::{
    Activate, CloseDetail, DASHBOARD_CONTEXT, SelectFirst, SelectLast, SelectNext, SelectPrevious,
};

mod heading;
pub use heading::header;

/// One group's lines: its header, then its rows and note unless folded.
/// The note shares the last line's element, so each line of the scroll
/// container stays one visible key.
fn one_group_lines<D: ListDashboard>(
    this: &D,
    group: &Group,
    tokens: &Tokens,
    cx: &mut Context<D>,
) -> Vec<AnyElement> {
    let folded = this.list().is_folded(&group.key);
    let mut lines = vec![group_row(this, group, folded, tokens, cx)];
    if folded {
        return lines;
    }
    lines.extend(group.rows.iter().map(|row| item_row(this, row, tokens, cx)));
    if let Some(note) = this.group_note(group, cx)
        && let Some(last) = lines.pop()
    {
        lines.push(with_note(this, last, group, note, tokens));
    }
    lines
}

/// The group headers and their rows, then the empty state when there are no
/// groups, and blank space under the last line.
fn group_lines<D: ListDashboard>(
    this: &D,
    tokens: &Tokens,
    cx: &mut Context<D>,
) -> Vec<AnyElement> {
    let list = this.list();
    let tokens = &row_tokens(tokens, this.comfortable_rows());
    let mut lines: Vec<AnyElement> = Vec::new();
    for group in list.groups() {
        lines.append(&mut one_group_lines(this, group, tokens, cx));
    }
    if list.groups().is_empty() {
        let (title, hint) = this.empty_message();
        let words = words(this, title, hint);
        let actions = this.empty_actions(cx).unwrap_or_else(|| {
            vec![feedback::configure_action("list-empty-configure").into_any_element()]
        });
        let empty = feedback::empty_words(tokens, "list-empty", words, actions);
        lines.push(empty.into_any_element());
    }
    lines.push(
        div()
            .h(px(LIST_TAIL_HEIGHT_PX))
            .flex_shrink_0()
            .into_any_element(),
    );
    lines
}

/// The tokens rows draw with: a comfortable list's rows are a step taller
/// than the theme's, so the density setting still scales them.
fn row_tokens(tokens: &Tokens, comfortable: bool) -> Tokens {
    if comfortable {
        Tokens {
            row_height: tokens.row_height * COMFORTABLE_ROW_SCALE,
            ..*tokens
        }
    } else {
        *tokens
    }
}

/// The scroll container's children, and how many of them come before the
/// first group line. The lines are its direct children, so the scroll
/// handle can bring the selected one into view by its position.
fn lines<D: ListDashboard>(
    this: &D,
    tokens: &Tokens,
    retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<D>,
) -> (Vec<AnyElement>, usize) {
    let list = this.list();
    let bare = list.groups().is_empty();
    match list.status() {
        LoadStatus::Loading if bare => {
            // The skeleton stands in for rows, so it takes their height.
            let rows = row_tokens(tokens, this.comfortable_rows());
            (vec![feedback::skeleton(&rows).into_any_element()], 0)
        }
        LoadStatus::Failed { title, message } if bare => {
            let actions = [
                feedback::retry_action("list-error-retry", retry),
                feedback::configure_action("list-error-configure"),
            ];
            let words = words(this, title.clone(), message.clone());
            let box_ = feedback::error_box_words(tokens, "list-error", words, actions);
            (vec![box_.into_any_element()], 0)
        }
        LoadStatus::Failed { title, message } => {
            let retry_id = feedback::text_key("list-banner", "banner", "retry", title);
            let words = words(this, title.clone(), message.clone());
            let banner = feedback::error_banner_words(
                tokens,
                "list-banner",
                words,
                [feedback::retry_action(retry_id, retry)],
            );
            let mut lines = vec![banner.into_any_element()];
            lines.append(&mut group_lines(this, tokens, cx));
            (lines, 1)
        }
        _ => (group_lines(this, tokens, cx), 0),
    }
}

/// A state's words, with the chat offered only where the dashboard allows.
fn words<D: ListDashboard>(this: &D, title: SharedString, message: SharedString) -> Words {
    Words {
        chat: this.menu_chat(),
        ..Words::new(title, message)
    }
}

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
    let tokens = Tokens::current(cx);
    let retry = cx.listener(|this: &mut D, _: &ClickEvent, window, cx| this.reload(window, cx));
    let (lines, first_row) = lines(this, &tokens, retry, cx);
    let list = this.list();
    scroll_to_selected(list, first_row);
    let panel = list
        .open_row()
        .and_then(|row| detail_panel(this, row, window, cx));

    let frame = h_flex()
        .id(D::ID)
        .key_context(key_context::<D>(cx.platform_info().os))
        .track_focus(&this.focus_handle(cx));
    with_list_keys(frame, cx)
        .size_full()
        .min_h_0()
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(
            v_flex()
                .id("list")
                .flex_1()
                .min_w(px(this.min_list_width()))
                .h_full()
                .overflow_y_scroll()
                .track_scroll(this.list().scroll_handle())
                .children(lines),
        )
        .children(panel.map(|panel| div().min_w_0().h_full().child(panel)))
}
mod rows;
use rows::{group_row, item_row, with_note};

mod panel_view;

use panel_view::detail_panel;
pub use panel_view::render_body;
