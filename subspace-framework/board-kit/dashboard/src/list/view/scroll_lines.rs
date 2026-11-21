//! The lines of a list's scroll container: the load states, the group
//! headers and their rows, and the blank space under the last line.

use gpui_kit::{AnyElement, App, ClickEvent, Context, IntoElement, SharedString, Styled, Window};
use gpui_kit::{div, px};

use super::super::{COMFORTABLE_ROW_SCALE, Group, LIST_TAIL_HEIGHT_PX, ListDashboard, LoadStatus};
use super::rows::{group_row, item_row, with_note};
use crate::feedback::{self, Words};
use crate::tokens::Tokens;

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
pub(super) fn lines<D: ListDashboard>(
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
