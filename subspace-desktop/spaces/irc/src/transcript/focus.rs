//! Keyboard focus in a transcript: Shift+Tab from the message input moves
//! into the newest message, ↑ and ↓ move between messages, Tab and
//! Shift+Tab between the links and references of the focused message,
//! Enter opens the focused one and Escape returns to the input. Pure, so
//! each step is tested without a window.

use std::collections::VecDeque;

use super::Row;
use crate::lines::Item;
use crate::state::Entry;

/// The focused message, by line number, and the index of its focused link
/// or reference in reading order, if one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranscriptFocus {
    pub seq: i64,
    pub reference: Option<usize>,
}

/// The line numbers of the message rows, oldest first.
pub(crate) fn messages(rows: &[Row], entries: &VecDeque<Entry>) -> Vec<i64> {
    rows.iter()
        .filter_map(|row| match row {
            Row::Item(Item::Message { at, .. }) => entries.get(*at).map(|entry| entry.seq),
            _ => None,
        })
        .collect()
}

/// The newest message, where focus enters the transcript.
pub(crate) fn newest(messages: &[i64]) -> Option<TranscriptFocus> {
    messages.last().map(|seq| TranscriptFocus {
        seq: *seq,
        reference: None,
    })
}

/// The message `delta` messages from `cursor`'s, stopping at either end.
/// A message that is gone, such as one trimmed from history, gives the
/// newest.
pub(crate) fn step_message(
    messages: &[i64],
    cursor: TranscriptFocus,
    delta: isize,
) -> Option<TranscriptFocus> {
    let Some(at) = messages.iter().position(|seq| *seq == cursor.seq) else {
        return newest(messages);
    };
    let last = messages.len().saturating_sub(1);
    let next = at.saturating_add_signed(delta).min(last);
    Some(TranscriptFocus {
        seq: messages[next],
        reference: None,
    })
}

/// The next (`forward`) or previous of the message's `count` links and
/// references. Shift+Tab from the first leaves only the message focused;
/// Tab past the last stays on it.
pub(crate) fn step_reference(
    cursor: TranscriptFocus,
    count: usize,
    forward: bool,
) -> TranscriptFocus {
    let reference = match (cursor.reference, forward) {
        (_, true) if count == 0 => None,
        (None, true) => Some(0),
        (Some(at), true) => Some((at + 1).min(count - 1)),
        (None | Some(0), false) => None,
        (Some(at), false) => Some(at.min(count) - 1),
    };
    TranscriptFocus {
        reference,
        ..cursor
    }
}

#[cfg(test)]
mod tests {
    use super::{TranscriptFocus, newest, step_message, step_reference};

    fn at(seq: i64, reference: Option<usize>) -> TranscriptFocus {
        TranscriptFocus { seq, reference }
    }

    #[test]
    fn focus_enters_at_the_newest_message_and_stops_at_the_ends() {
        let messages = [3, 5, 9];
        assert_eq!(newest(&messages), Some(at(9, None)));
        assert_eq!(newest(&[]), None);
        assert_eq!(
            step_message(&messages, at(9, Some(1)), -1),
            Some(at(5, None))
        );
        assert_eq!(step_message(&messages, at(3, None), -1), Some(at(3, None)));
        assert_eq!(step_message(&messages, at(9, None), 1), Some(at(9, None)));
        assert_eq!(step_message(&messages, at(4, None), 1), Some(at(9, None)));
    }

    #[test]
    fn tab_walks_the_message_s_references() {
        let start = at(1, None);
        let first = step_reference(start, 2, true);
        assert_eq!(first, at(1, Some(0)));
        let second = step_reference(first, 2, true);
        assert_eq!(second, at(1, Some(1)));
        assert_eq!(step_reference(second, 2, true), second, "stays on the last");
        assert_eq!(step_reference(second, 2, false), first);
        assert_eq!(
            step_reference(first, 2, false),
            start,
            "back to the message"
        );
        assert_eq!(step_reference(start, 0, true), start, "none to walk");
        assert_eq!(step_reference(at(1, Some(4)), 2, false), at(1, Some(1)));
    }
}
