//! Which row is open as a tab beside the list, and which of the two tabs
//! the pane shows. No GPUI and no requests.

/// The list's tab, which the strip always has first.
pub(crate) const LIST_TAB: usize = 0;
/// The opened item's tab, beside the list's.
pub(crate) const ITEM_TAB: usize = 1;

/// The opened row and whether its tab is the one on show.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OpenItem {
    /// The opened row's key. Its tab stays until it is closed; opening
    /// another row replaces it.
    pub(crate) key: Option<String>,
    /// Whether the pane shows the opened row rather than the list.
    pub(crate) shown: bool,
}

impl OpenItem {
    /// Opens row `key` and shows its tab.
    pub(crate) fn open(&mut self, key: &str) {
        self.key = Some(key.to_owned());
        self.shown = true;
    }

    /// Closes the opened row's tab; the pane shows the list again.
    pub(crate) fn close(&mut self) {
        *self = Self::default();
    }

    /// Shows tab `index`. The item's tab shows only while a row is open.
    pub(crate) fn show(&mut self, index: usize) {
        self.shown = index == ITEM_TAB && self.key.is_some();
    }

    /// The key of the row whose tab is on show, if the item's tab is.
    pub(crate) fn showing(&self) -> Option<&str> {
        self.key.as_deref().filter(|_| self.shown)
    }

    /// The tab strip's selected index.
    pub(crate) fn tab_index(&self) -> usize {
        if self.showing().is_some() {
            ITEM_TAB
        } else {
            LIST_TAB
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ITEM_TAB, LIST_TAB, OpenItem};

    #[test]
    fn opening_shows_the_item_tab_and_the_list_tab_hides_it_without_closing() {
        let mut item = OpenItem::default();
        assert_eq!(item.showing(), None);
        assert_eq!(item.tab_index(), LIST_TAB);
        item.open("cloud:a");
        assert_eq!(item.showing(), Some("cloud:a"));
        assert_eq!(item.tab_index(), ITEM_TAB);
        item.show(LIST_TAB);
        assert_eq!(item.showing(), None);
        assert_eq!(item.key.as_deref(), Some("cloud:a"), "the tab stays open");
        item.show(ITEM_TAB);
        assert_eq!(item.showing(), Some("cloud:a"));
    }

    #[test]
    fn opening_another_row_replaces_the_tab_and_closing_returns_to_the_list() {
        let mut item = OpenItem::default();
        item.open("cloud:a");
        item.open("local:b");
        assert_eq!(item.showing(), Some("local:b"));
        item.close();
        assert_eq!(item, OpenItem::default());
        item.show(ITEM_TAB);
        assert_eq!(item.showing(), None, "no tab to show once closed");
    }
}
