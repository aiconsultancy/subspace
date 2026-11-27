//! A banded group header's look, kept apart so its one-line-per-edge rule
//! has its own test.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Div, Stateful, Styled as _};

use crate::tokens::Tokens;

/// A banded group header: the filled band, and the stronger line under it
/// that the row frame draws. No line above: that edge is the line before.
pub(super) fn band(row: Stateful<Div>, selected: bool, tokens: &Tokens) -> Stateful<Div> {
    row.when(!selected, |row| row.bg(tokens.pane))
        .border_color(tokens.line)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{SharedString, Styled as _};

    use super::super::row_frame;
    use super::band;
    use crate::tokens::Tokens;

    #[test]
    fn a_banded_group_header_draws_only_the_line_under_it() {
        let tokens = Tokens::one_dark();
        assert!(!tokens.inset_rows(), "One Dark bands its group headers");
        let key = SharedString::from("group:open");
        let mut header = band(row_frame(&key, false, false, &tokens), false, &tokens);
        let borders = &header.style().border_widths;
        assert!(borders.top.is_none(), "no line above: {borders:?}");
        assert!(borders.bottom.is_some(), "the line under it");
    }
}
