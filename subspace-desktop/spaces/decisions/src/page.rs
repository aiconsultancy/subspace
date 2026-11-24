//! A decision opened as a page: its own tab beside the list, read in full
//! from Subspace API, until it is closed.

mod controls;
pub(crate) mod view;

use gpui_kit::{AppContext as _, Context, Window};
use subspace_api_client::Error;
use subspace_api_client::model::DecisionDetail;
use subspace_framework::dashboard::ReturnLocation;

use crate::load::{self, Failure, Leased};
use crate::{Decisions, peek};

/// A Back from another Space returns to the page with this key, then the
/// account epoch and the decision's id.
const RETURN_PREFIX: &str = "decision:";
/// A Back to the list returns to the decision selected there with this
/// key, then the account epoch and the decision's id.
const SELECTED_PREFIX: &str = "decision-row:";

/// The open decision and how its read went.
pub(crate) struct Page {
    pub(crate) id: String,
    /// The row's title until the read lands, then the decision's own.
    pub(crate) title: String,
    pub(crate) state: PageState,
}

pub(crate) enum PageState {
    Loading,
    Loaded(Box<DecisionDetail>),
    /// The decision no longer exists, or the account may not read it.
    Gone,
    Failed(Failure),
}

fn page_state(result: Result<DecisionDetail, Error>) -> PageState {
    match result {
        Ok(detail) => PageState::Loaded(Box::new(detail)),
        Err(Error::NotFound) => PageState::Gone,
        Err(error) => PageState::Failed(load::failure(error)),
    }
}

/// The open page as a place a Back on another Space returns to.
pub(crate) fn return_location(decisions: &Decisions) -> Option<ReturnLocation> {
    let page = decisions.shown_page()?;
    Some(place(decisions, RETURN_PREFIX, &page.id, &page.title))
}

/// Where a Back from a linked issue returns: the open page, or in the list
/// the decision whose inspector the issue was chosen from.
pub(crate) fn issue_origin(decisions: &Decisions) -> Option<ReturnLocation> {
    return_location(decisions).or_else(|| {
        let item = decisions.selected_item()?;
        Some(place(decisions, SELECTED_PREFIX, &item.id, &item.title))
    })
}

/// A return location for decision `id` under the current account epoch.
fn place(decisions: &Decisions, prefix: &str, id: &str, title: &str) -> ReturnLocation {
    ReturnLocation {
        label: title.to_owned().into(),
        key: format!("{prefix}{}:{id}", decisions.account_epoch).into(),
    }
}

/// The decision id in `key` after `prefix`, when the key was given under
/// account epoch `epoch`.
fn returning<'a>(key: &'a str, prefix: &str, epoch: u64) -> Option<&'a str> {
    let (given, id) = key.strip_prefix(prefix)?.split_once(':')?;
    (given.parse::<u64>().ok() == Some(epoch) && !id.is_empty()).then_some(id)
}

// qual:allow(coupling, oi) reason: "Opening, reading and closing the page change the Space's pane and tab strip together."
impl Decisions {
    /// The page, while the pane shows it.
    pub(crate) fn shown_page(&self) -> Option<&Page> {
        self.page.as_ref().filter(|_| self.showing_page)
    }

    pub(crate) fn page_loading(&self) -> bool {
        self.page
            .as_ref()
            .is_some_and(|page| matches!(page.state, PageState::Loading))
    }

    /// Opens decision `id` as the page, in place of any other.
    pub(crate) fn open_page(&mut self, id: String, title: String, cx: &mut Context<Self>) {
        if self.follow_account(cx) {
            return;
        }
        self.history = Default::default();
        self.page = Some(Page {
            id,
            title,
            state: PageState::Loading,
        });
        self.showing_page = true;
        self.read_page(cx);
        cx.notify();
    }

    /// Reads the page's decision, again on Retry and Reload.
    pub(crate) fn read_page(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.page.as_mut() else {
            return;
        };
        page.state = PageState::Loading;
        let id = page.id.clone();
        self.page_reads = self.page_reads.wrapping_add(1);
        let number = self.page_reads;
        let reader = self.reader.detail.clone();
        let lease = self.lease.clone();
        let read = cx.background_spawn(async move { reader(&id, lease.as_ref()) });
        self._page_read = Some(cx.spawn(async move |this, cx| {
            let result = read.await;
            _ = this.update(cx, |this, cx| this.finish_page(number, result, cx));
        }));
        cx.notify();
    }

    fn finish_page(
        &mut self,
        number: u64,
        result: Leased<Result<DecisionDetail, Error>>,
        cx: &mut Context<Self>,
    ) {
        let (lease, result) = result.split(Err);
        let wanted = number == self.page_reads && self.page.is_some();
        if !self.landed(wanted, lease.as_ref(), cx) {
            return;
        }
        let Some(page) = self.page.as_mut() else {
            return;
        };
        page.state = page_state(result);
        if let PageState::Loaded(detail) = &page.state {
            page.title.clone_from(&detail.title);
        }
        cx.notify();
    }

    /// Closes the page and its tab, and shows the list with the decision
    /// still selected. Escape and the tab's Close.
    pub(crate) fn close_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page.is_none() {
            return;
        }
        self.page = None;
        self.history = Default::default();
        self.showing_page = false;
        self._page_read = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Shows the list, keeping an open page's tab.
    pub(crate) fn show_list(&mut self, cx: &mut Context<Self>) {
        if self.showing_page {
            self.showing_page = false;
            cx.notify();
        }
    }

    /// A click on the tab strip: the list's tab, or the page's.
    pub(crate) fn show_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.follow_account(cx);
        self.showing_page = index > 0 && self.page.is_some();
        cx.notify();
    }

    /// The issues the open page's decision links to, once it is read.
    pub(crate) fn page_issues(&self) -> Vec<peek::Issue> {
        match self.shown_page().map(|page| &page.state) {
            Some(PageState::Loaded(detail)) => {
                peek::relations(self.history.snapshot().unwrap_or(detail)).issues
            }
            _ => Vec::new(),
        }
    }

    /// A Back from another Space: the page it was opened from, or the
    /// decision selected in the list. A place given before an account
    /// change no longer exists.
    pub(crate) fn return_to_place(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let epoch = self.account_epoch;
        if let Some(id) = returning(key, RETURN_PREFIX, epoch) {
            let id = id.to_owned();
            self.open_decision(&id, cx);
            return true;
        }
        let Some(id) = returning(key, SELECTED_PREFIX, epoch) else {
            return false;
        };
        let id = id.to_owned();
        self.show_list(cx);
        // The list may since have been read without it; the list still shows.
        if let Some(item) = self.item(&id) {
            let title = item.title.clone();
            self.follow(&id, &title, cx);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use subspace_api_client::Error;

    use super::{PageState, RETURN_PREFIX, SELECTED_PREFIX, page_state, returning};
    use crate::fixtures;
    use crate::load::Failure;

    #[test]
    fn a_return_key_names_its_account_epoch() {
        assert_eq!(returning("decision:3:d2", RETURN_PREFIX, 3), Some("d2"));
        assert_eq!(
            returning("decision-row:0:d2", SELECTED_PREFIX, 0),
            Some("d2")
        );
        assert_eq!(
            returning("decision:2:d2", RETURN_PREFIX, 3),
            None,
            "an earlier account"
        );
        assert_eq!(returning("decision:d2", RETURN_PREFIX, 0), None);
        assert_eq!(returning("decision-row:0:d2", RETURN_PREFIX, 0), None);
        assert_eq!(returning("decision:0:", RETURN_PREFIX, 0), None);
    }

    #[test]
    fn a_read_becomes_the_page_s_state() {
        let detail = fixtures::detail("d2").unwrap();
        assert!(matches!(page_state(Ok(detail)), PageState::Loaded(_)));
        assert!(matches!(page_state(Err(Error::NotFound)), PageState::Gone));
        assert!(matches!(
            page_state(Err(Error::Network)),
            PageState::Failed(Failure::Network)
        ));
    }
}
