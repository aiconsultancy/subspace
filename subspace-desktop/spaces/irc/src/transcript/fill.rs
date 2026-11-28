//! A transcript row's fill at rest and under the pointer, as list rows
//! have. Only the selected thread or search result keeps a fill at rest.
//! Inline mentions and the unread divider mark attention in the transcript.

use gpui_kit::{Hsla, InteractiveElement, Styled};
use subspace_framework::dashboard::Tokens;

/// How strongly the foreground tints the row under the pointer.
const HOVER_TINT: f32 = 0.06;
const SELECTED_TINT: f32 = 0.12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RowFill {
    rest: Option<Hsla>,
    hover: Option<Hsla>,
}

impl RowFill {
    pub fn of(selected: bool, tokens: &Tokens) -> Self {
        if selected {
            Self {
                rest: Some(tokens.fg.opacity(SELECTED_TINT)),
                hover: None,
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

    use super::{HOVER_TINT, RowFill, SELECTED_TINT};

    #[gpui_kit::test]
    fn row_fill_and_text_selection_remain_distinct(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let tokens = Tokens::current(cx);
            let active = RowFill::of(true, &tokens);
            let selection = subspace_framework::dashboard::rich::text_view_style(cx).selection();
            assert_ne!(active.rest, Some(selection));
            assert_ne!(RowFill::of(false, &tokens).hover, Some(selection));
        });
    }

    #[test]
    fn ordinary_rows_only_fill_under_the_pointer_and_selected_rows_keep_their_fill() {
        let tokens = Tokens::one_dark();
        let plain = RowFill::of(false, &tokens);
        assert_eq!(plain.rest, None, "a plain row is clear at rest");
        assert_eq!(
            plain.hover,
            Some(tokens.fg.opacity(HOVER_TINT)),
            "a light foreground tint"
        );
        let selected = RowFill::of(true, &tokens);
        assert_eq!(selected.rest, Some(tokens.fg.opacity(SELECTED_TINT)));
        assert_eq!(selected.hover, None);
    }
}
