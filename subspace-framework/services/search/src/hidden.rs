//! Records a board hides for a while without deleting them: from every
//! search, while candidates are selected and again before a result shows,
//! from related items, and from reading a result by its citation. Only the
//! board's own record pages ([`Service::records`]) still return them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{Document, Service, storage_error};

/// The boards' rules, by board.
pub(crate) type Rules = BTreeMap<String, crate::service::Hidden>;

// qual:allow(coupling, oi) reason: "Hiding records is one concern of the service, kept apart from its stores and writes in service.rs."
impl Service {
    /// Hides `board`'s documents for which `hidden` answers true from every
    /// search, related lookup and citation read, asked again each time and
    /// by [`Self::shows`], as for a board's records that belong to an
    /// account other than the one signed in. Nothing is deleted. A later
    /// call replaces the rule.
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
        !self.hides(&hit.target.scope.board, &hit.document)
    }

    /// Whether `board`'s rule hides `document` now; everything is hidden
    /// while the rules cannot be read.
    pub(crate) fn hides(&self, board: &str, document: &Document) -> bool {
        self.rules()
            .map_or(true, |rules| hides(&rules, board, document))
    }

    /// The rules as they are now, to ask without holding their lock.
    pub(crate) fn rules(&self) -> Result<Rules, String> {
        Ok(self.inner.hidden.read().map_err(storage_error)?.clone())
    }
}

/// Whether `rules` hide `board`'s `document`.
pub(crate) fn hides(rules: &Rules, board: &str, document: &Document) -> bool {
    rules.get(board).is_some_and(|hidden| hidden(document))
}
