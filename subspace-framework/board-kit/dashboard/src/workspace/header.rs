//! The header strip at the top of a dock. It is as tall as the pane's tab
//! strip, so the hairlines under them meet across the workspace.

use gpui_kit::component::h_flex;
use gpui_kit::{
    Div, FontWeight, Hsla, InteractiveElement as _, ParentElement as _, Pixels, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, px,
};

use super::text_edge;
use crate::tokens::Tokens;

/// Room after the last header control, which carries its own padding.
const TRAILING_PX: f32 = 8.;

/// The strip itself: the text edge on the leading side and a hairline below.
/// It is exactly [`strip_height`](super::strip_height) high, hairline
/// included, as the pane's tab strip is, so a dock's header never grows past
/// its neighbours.
pub(super) fn strip(
    id: &'static str,
    line: Hsla,
    height: Pixels,
    tokens: &Tokens,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .h(height)
        .gap_1()
        .pl(text_edge(tokens))
        .pr(px(TRAILING_PX))
        .border_b_1()
        .border_color(line)
}

/// A section label as it shows: in capitals. Its accessible name and id keep
/// the text as written.
pub(super) fn capitals(label: &str) -> SharedString {
    label.to_uppercase().into()
}

/// A dock's title: one line that takes the room the controls leave.
pub(super) fn title(
    id: &'static str,
    text: SharedString,
    weight: FontWeight,
    family: SharedString,
    tokens: &Tokens,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_label(text.clone())
        .flex_1()
        .min_w_0()
        .truncate()
        .font_family(family)
        .font_weight(weight)
        .text_size(tokens.row_font)
        .text_color(tokens.fg_strong)
        .child(text)
}
