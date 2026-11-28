//! Stable row identities turned into native virtual-list splices, so new
//! lines and prepended history keep the reader's place.

use std::collections::VecDeque;
use std::ops::Range;

use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::{App, AppContext as _, Entity};
use subspace_client_irc::BufferKey;

use crate::hub::Hub;
use crate::lines::{self, Item};
use crate::state::Entry;
use crate::view::HISTORY_PAGE;

/// One scroller row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    /// The control that asks for older history.
    History,
    /// The start of the unread messages, anchored to the first message.
    UnreadDivider(i64),
    /// An entry point for replies whose original message is older than the transcript.
    EarlierThread(usize),
    Item(Item),
}

/// One conversation's scroller: its rows, their identities and the native
/// virtual list's state, which keeps the reader's place.
pub(crate) struct Scroller {
    pub state: Entity<MessageScrollerState>,
    rows: Vec<Row>,
    ids: Vec<String>,
}

impl Scroller {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: cx.new(|cx| MessageScrollerState::new(0, cx)),
            rows: Vec::new(),
            ids: Vec::new(),
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn row(&self, index: usize) -> Option<Row> {
        self.rows.get(index).copied()
    }

    /// Metadata can change a surviving row's height without replacing it.
    pub fn remeasure(&self, indices: &[usize], cx: &mut App) {
        self.state.update(cx, |state, cx| {
            for index in indices {
                _ = state.remeasure_items(*index..*index + 1, cx);
            }
        });
    }

    /// Shows `rows` from the latest one.
    pub fn reset(&mut self, rows: Vec<Row>, ids: Vec<String>, cx: &mut App) {
        self.ids = ids;
        self.rows = rows;
        let count = self.rows.len();
        self.state.update(cx, |state, cx| {
            state.reset(count, cx);
            state.scroll_to_end(cx);
        });
    }

    /// Applies new rows as one splice.
    pub fn replace(&mut self, rows: Vec<Row>, ids: Vec<String>, cx: &mut App) {
        let change = change(&self.ids, &ids);
        self.ids = ids;
        self.rows = rows;
        if let Some((range, count)) = change {
            self.state
                .update(cx, |state, cx| _ = state.splice(range, count, cx));
        }
    }
}

/// The stable identity of each row.
pub(crate) fn identities(rows: &[Row], entries: &VecDeque<Entry>) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::History => "history".to_owned(),
            Row::UnreadDivider(seq) => format!("unread:{seq}"),
            Row::EarlierThread(at) => format!("earlier-thread:{}", entries[*at].seq),
            Row::Item(item) => lines::item_id(*item, entries),
        })
        .collect()
}

/// The scroller row that shows the entry at index `at`.
pub(crate) fn row_of(rows: &[Row], at: usize) -> Option<usize> {
    rows.iter().position(|row| match row {
        Row::Item(Item::Message { at: shown, .. } | Item::Line(shown)) => *shown == at,
        Row::EarlierThread(shown) => *shown == at,
        Row::Item(Item::Membership { first, last }) => (*first..=*last).contains(&at),
        Row::Item(Item::Day(_)) | Row::History | Row::UnreadDivider(_) => false,
    })
}

/// The one splice that turns `old` into `new`: everything between their
/// common start and common end. `None` when nothing changed.
pub(crate) fn change(old: &[String], new: &[String]) -> Option<(Range<usize>, usize)> {
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(left, right)| left == right)
        .count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(left, right)| left == right)
        .count();
    let removed = prefix..old.len() - suffix;
    let added = new.len() - prefix - suffix;
    (!removed.is_empty() || added > 0).then_some((removed, added))
}

/// Asks for the page of history before the oldest line held.
pub(crate) fn load_older(hub: &Entity<Hub>, buffer: &BufferKey, cx: &mut App) {
    hub.update(cx, |hub, cx| {
        if let Some(command) = hub.state.older_history(buffer, HISTORY_PAGE) {
            hub.send(command);
            cx.notify();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{Row, change, row_of};
    use crate::lines::Item;

    #[test]
    fn an_entry_is_found_in_its_message_line_or_membership_row() {
        let rows = [
            Row::History,
            Row::Item(Item::Day(chrono::NaiveDate::default())),
            Row::Item(Item::Message {
                at: 0,
                continues: false,
            }),
            Row::Item(Item::Membership { first: 1, last: 3 }),
            Row::Item(Item::Line(4)),
        ];
        assert_eq!(row_of(&rows, 0), Some(2));
        assert_eq!(row_of(&rows, 2), Some(3));
        assert_eq!(row_of(&rows, 4), Some(4));
        assert_eq!(row_of(&rows, 5), None);
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn appended_lines_and_prepended_history_are_single_splices() {
        let old = ids(&["history", "day:1", "line:1", "line:2"]);
        assert_eq!(
            change(
                &old,
                &ids(&["history", "day:1", "line:1", "line:2", "line:3"])
            ),
            Some((4..4, 1))
        );
        assert_eq!(
            change(
                &old,
                &ids(&["history", "day:0", "line:-1", "day:1", "line:1", "line:2"])
            ),
            Some((1..1, 2))
        );
        assert_eq!(change(&old, &old), None);
    }

    #[test]
    fn a_grown_membership_run_replaces_only_its_row() {
        let old = ids(&["day:1", "line:1", "members:2:2"]);
        let new = ids(&["day:1", "line:1", "members:2:3"]);
        assert_eq!(change(&old, &new), Some((2..3, 1)));
        let complete = ids(&["day:1", "line:1", "members:2:2"]);
        assert_eq!(
            change(
                &ids(&["history", "day:1", "line:1", "members:2:2"]),
                &complete
            ),
            Some((0..1, 0))
        );
    }
}
