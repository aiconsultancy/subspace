//! Reaction controls for replies displayed only in the inspector.

use gpui_kit::AnyElement;

use super::{Entry, RowContext, add_control, choices, frame, row};

/// A reply displayed only in the inspector keeps its reaction controls.
pub(crate) fn thread_reply(entry: &Entry, context: &RowContext) -> Option<AnyElement> {
    if let Some(row) = row(entry, context) {
        return Some(row);
    }
    let msgid = entry.line.id.as_deref().filter(|_| context.can_react)?;
    let add = add_control(entry.seq, choices(&[], None), msgid, &context.conversation);
    Some(frame(entry.seq, "irc-thread-line", std::iter::once(add)))
}
