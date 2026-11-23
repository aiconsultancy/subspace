//! One completion choice and the choices for a word.

use std::ops::Range;

use subspace_framework::dashboard::chat_input::completion::CompletionItem;

/// One choice: the row shown, and the text that replaces the word, or,
/// for a tracker command's first row, running the draft as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Suggestion {
    pub item: CompletionItem,
    pub insert: String,
    /// Choosing it sends the draft instead of completing a word.
    pub run: bool,
    /// For a run row, what it said it would do and for which tracker: the
    /// draft runs only while a fresh row says the same.
    pub shows: String,
}

impl Suggestion {
    /// A choice that replaces the word with `insert`.
    pub(crate) fn insert(item: CompletionItem, insert: String) -> Self {
        Self {
            item,
            insert,
            run: false,
            shows: String::new(),
        }
    }

    /// The row that runs the draft, as Return would, having said `shows`.
    pub(crate) fn run(item: CompletionItem, shows: String) -> Self {
        Self {
            item,
            insert: String::new(),
            run: true,
            shows,
        }
    }
}

/// The choices for the word in `range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Offer {
    pub range: Range<usize>,
    pub choices: Vec<Suggestion>,
}
