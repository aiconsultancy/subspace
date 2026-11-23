//! Records a board hides from search for a while without deleting them.

use std::sync::Arc;

use crate::{Document, Service, storage_error};

// qual:allow(coupling, oi) reason: "Hiding records is one concern of the service, kept apart from its stores and writes in service.rs."
impl Service {
    /// Hides `board`'s documents for which `hidden` answers true from every
    /// search, asked again at each search and by [`Self::shows`], as for a
    /// board's records that belong to an account other than the one signed
    /// in. Nothing is deleted. A later call replaces the rule.
    // qual:api - apps set it for their boards; the Framework only asks it.
    pub fn set_hidden(
        &self,
        board: &str,
        hidden: impl Fn(&Document) -> bool + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.inner
            .hidden
            .write()
            .map_err(storage_error)?
            .insert(board.to_owned(), Arc::new(hidden));
        Ok(())
    }

    /// Whether `hit` may be shown now, by its board's rule: a result kept
    /// from an earlier search is asked again before it is shown.
    pub fn shows(&self, hit: &crate::Hit) -> bool {
        let Ok(hidden) = self.inner.hidden.read() else {
            return false;
        };
        hidden
            .get(&hit.target.scope.board)
            .is_none_or(|hidden| !hidden(&hit.document))
    }
}
