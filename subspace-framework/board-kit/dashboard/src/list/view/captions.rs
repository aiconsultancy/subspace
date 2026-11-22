//! A group's captions: quiet labels inside the group, each above the row it
//! names, such as SHIPS and PEOPLE. A caption rides on its row's element,
//! so the scroll container keeps one line per visible key.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt as _, px,
};

use super::super::{GroupCaption, ListDashboard, ROW_PADDING_LEFT_PX};
use crate::tokens::Tokens;

/// A caption inside a group: a quiet label in capitals, indented like the
/// group's rows, which the selection skips. Its accessible name keeps the
/// text as written.
fn caption_line<D: ListDashboard>(
    this: &D,
    caption: &GroupCaption,
    tokens: &Tokens,
) -> impl IntoElement {
    let left = px(ROW_PADDING_LEFT_PX) + tokens.row_inset + tokens.indent;
    h_flex()
        .id(SharedString::from(format!(
            "list-caption:{}",
            caption.before
        )))
        .test_support()
        .role(Role::Heading)
        .aria_label(caption.label.clone())
        .w_full()
        .flex_shrink_0()
        .px(tokens.pad_x + tokens.row_inset)
        .pl(left)
        .when_some(this.list_side_inset(), |line, inset| {
            line.pr(inset).pl(inset + tokens.indent)
        })
        .pt_2()
        .pb_1()
        .text_size(tokens.caption_size())
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.muted)
        .child(caption.label.to_uppercase())
}

/// A row with its group's caption above it, drawn as one line.
pub(super) fn with_caption<D: ListDashboard>(
    this: &D,
    row: AnyElement,
    caption: &GroupCaption,
    tokens: &Tokens,
) -> AnyElement {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .child(caption_line(this, caption, tokens))
        .child(row)
        .into_any_element()
}
