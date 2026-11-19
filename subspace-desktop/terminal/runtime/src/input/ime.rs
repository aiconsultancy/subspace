//! Pre-edit text from a platform input method (REQ-18).
//!
//! A composition arrives as marked text the user has not committed yet. The
//! session must not see it, so it is held here and drawn at the cursor
//! instead; only the committed text reaches the encoder.
//!
//! # Pre-edit wider than the row
//!
//! `delivery.md` leaves open how pre-edit text wider than the columns left on
//! the cursor's row is drawn. It is clipped to those columns, as the Ghostty
//! and Kitty terminals do: the candidate window the input method draws itself
//! carries the whole composition, so the clipped tail costs the user nothing,
//! while wrapping it would move the rows of a program's own screen.

/// The marked text of a composition in progress.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PreEdit {
    text: String,
}

impl PreEdit {
    /// Hold new marked text, which replaces whatever the composition showed
    /// before.
    pub(crate) fn mark(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
    }

    /// Drop the composition, which a commit or a cancellation ends.
    pub(crate) fn clear(&mut self) {
        self.text.clear();
    }

    /// Whether a composition is in progress.
    pub(crate) fn is_active(&self) -> bool {
        !self.text.is_empty()
    }

    /// The marked text itself.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

/// The part of a composition that fits the columns left on its row.
pub(crate) fn clipped(text: &str, columns: usize) -> &str {
    match text.char_indices().nth(columns) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use crate::input::ime::{PreEdit, clipped};

    #[test]
    fn marked_text_replaces_the_last_composition_and_a_commit_ends_it() {
        let mut pre_edit = PreEdit::default();
        assert!(!pre_edit.is_active());

        pre_edit.mark("に");
        pre_edit.mark("にほ");
        let composing = pre_edit.text().to_owned();
        pre_edit.clear();

        assert_eq!(composing, "にほ");
        assert_eq!(pre_edit.text(), "");
        assert!(!pre_edit.is_active());
    }

    #[test]
    fn a_composition_is_clipped_to_the_columns_left_on_its_row() {
        assert_eq!(clipped("にほんご", 2), "にほ");
        assert_eq!(clipped("にほんご", 4), "にほんご");
        assert_eq!(clipped("にほんご", 9), "にほんご");
        assert_eq!(clipped("にほんご", 0), "");
    }
}
