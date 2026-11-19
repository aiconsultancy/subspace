//! The message bubble shared by Copilot and messaging boards.

use crate::Tokens;
use gpui_kit::component::bubble::{Bubble, BubbleContent, BubbleVariant};
use gpui_kit::component::message::{Message, MessageAlignment, MessageContent};
use gpui_kit::{ElementId, Role, Styled as _, relative};

const MAX_WIDTH_FRACTION: f32 = 0.85;

pub fn bubble(tokens: &Tokens) -> Bubble {
    Bubble::new()
        .with_variant(BubbleVariant::Secondary)
        .max_w(relative(MAX_WIDTH_FRACTION))
        .content(
            BubbleContent::new()
                .px_2()
                .py_1()
                .border_0()
                .rounded(tokens.surface_radius)
                .text_size(tokens.row_font)
                .text_color(tokens.fg)
                .bg(tokens.select),
        )
}

/// A native message with stable identity and caller-owned content slots.
/// Add headers, avatars and footers with the returned Kit builder.
// qual:api - native composition; no service or transcript state is owned here.
pub fn message(
    id: impl Into<ElementId>,
    alignment: MessageAlignment,
    content: MessageContent,
) -> Message {
    Message::new()
        .id(id)
        .role(Role::ListItem)
        .alignment(alignment)
        .content(content)
}
