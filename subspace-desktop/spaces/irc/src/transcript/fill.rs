//! A transcript row's fill at rest and under the pointer, as list rows
//! have. Mentions keep the selection fill. Unread messages have a quieter
//! accent tint, and read messages get a foreground tint under the pointer.

use gpui_kit::{Hsla, InteractiveElement, Styled};
use subspace_framework::dashboard::Tokens;

/// How strongly the foreground tints the row under the pointer.
const HOVER_TINT: f32 = 0.06;
const UNREAD_TINT: f32 = 0.12;
const UNREAD_HOVER_TINT: f32 = 0.18;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RowFill {
    rest: Option<Hsla>,
    hover: Option<Hsla>,
}

impl RowFill {
    pub fn of(highlight: bool, unread: bool, tokens: &Tokens) -> Self {
        if highlight {
            Self {
                rest: Some(tokens.select),
                hover: None,
            }
        } else if unread {
            Self {
                rest: Some(tokens.accent.opacity(UNREAD_TINT)),
                hover: Some(tokens.accent.opacity(UNREAD_HOVER_TINT)),
            }
        } else {
            Self {
                rest: None,
                hover: Some(tokens.fg.opacity(HOVER_TINT)),
            }
        }
    }
}

/// `row` with `fill`.
pub(super) fn filled<E: Styled + InteractiveElement>(mut row: E, fill: RowFill) -> E {
    if let Some(rest) = fill.rest {
        row = row.bg(rest);
    }
    if let Some(hover) = fill.hover {
        row = row.hover(move |style| style.bg(hover));
    }
    row
}

#[cfg(test)]
mod tests {
    use subspace_framework::dashboard::Tokens;

    use super::{HOVER_TINT, RowFill};

    #[gpui_kit::test]
    fn unread_fill_and_text_selection_remain_distinct(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let tokens = Tokens::current(cx);
            let unread = RowFill::of(false, true, &tokens);
            let selection = subspace_framework::dashboard::rich::text_view_style(cx).selection();
            assert_ne!(unread.rest, Some(selection));
            assert_ne!(unread.hover, Some(selection));
        });
    }

    #[test]
    fn rows_fill_under_the_pointer_and_mentions_keep_their_fill() {
        let tokens = Tokens::one_dark();
        let plain = RowFill::of(false, false, &tokens);
        assert_eq!(plain.rest, None, "a plain row is clear at rest");
        assert_eq!(
            plain.hover,
            Some(tokens.fg.opacity(HOVER_TINT)),
            "a light foreground tint"
        );
        let unread = RowFill::of(false, true, &tokens);
        assert!(unread.rest.is_some());
        assert_ne!(unread.rest, unread.hover);
        let mention = RowFill::of(true, true, &tokens);
        assert_eq!(mention.rest, Some(tokens.select));
        assert_eq!(
            mention.hover, None,
            "like a selected list row, a mention keeps its fill"
        );
    }
}
