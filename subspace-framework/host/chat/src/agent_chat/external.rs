//! What the app shell adds to the chat from outside the panel: the selected
//! row and its open details, or text selected in a detail pane.
//!
//! The shell owns the key, the panel button and the pane's context menu, so
//! this is the one way in. Pinning a reference here never sends a message:
//! the reference rides with the user's next prompt, like the automatic one.

use super::logic::upsert_context;
use super::{ChatPanel, Context};

use subspace_agent::ContextRef;

impl ChatPanel {
    /// Pins `reference` to the chat shown, replacing one with the same key.
    // qual:api - the app shell hands a context reference to the open chat.
    pub fn attach_reference(&mut self, reference: ContextRef, cx: &mut Context<Self>) {
        upsert_context(&mut self.tab_mut().context, reference);
        cx.notify();
    }
}
