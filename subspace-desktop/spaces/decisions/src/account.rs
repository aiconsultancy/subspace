//! Following the signed-in account: what the Space holds was read under a
//! [`Lease`] on an account and organisation, and once either changes, or
//! the person signs out, the Space drops all of it and reads again for the
//! new account, as the Issues Space restarts its hub.

use std::time::Duration;

use gpui_kit::{Context, Task, Window};
use subspace_framework::dashboard::ListState;

use crate::Decisions;
use crate::board::Folds;
use crate::load::Lease;
use crate::state::{Listing, Session};

/// How often the Space looks for an account or organisation change, as the
/// Issues Space's hub poll does, so nothing read for the previous account
/// stays on screen.
const ACCOUNT_WATCH: Duration = Duration::from_millis(250);

/// Looks for an account or organisation change for as long as the Space
/// lives, shown or hidden.
pub(crate) fn watch(window: &mut Window, cx: &mut Context<Decisions>) -> Task<()> {
    cx.spawn_in(window, async move |this, cx| {
        loop {
            cx.background_executor().timer(ACCOUNT_WATCH).await;
            let watched = this.update_in(cx, |this, window, cx| {
                this.follow_account(cx);
                this.tidy(window, cx);
            });
            if watched.is_err() {
                return;
            }
        }
    })
}

// qual:allow(coupling, oi) reason: "Following the account drops the state every other part of the Space reads; the rule it follows, a lease that moved, is tested through the fixtures' accounts."
impl Decisions {
    /// Whether an answer read under `lease` may be shown: it is still
    /// `wanted`, and neither it nor what the Space holds was read for an
    /// account that has since changed. An answer read for a previous account
    /// drops everything, as the account change does. An answer without a
    /// lease, a read that could make no client, read nothing for any
    /// account: it may show its refusal, and the Space keeps watching the
    /// account what it holds was read for.
    pub(crate) fn landed(
        &mut self,
        wanted: bool,
        lease: Option<&Lease>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.follow_account(cx) || !wanted {
            return false;
        }
        let Some(lease) = lease else {
            return true;
        };
        if lease.moved() {
            self.forget_account(cx);
            return false;
        }
        self.lease = Some(lease.clone());
        true
    }

    /// Drops what the Space holds once the account or organisation it was
    /// read for is no longer signed in. True when it did.
    pub(crate) fn follow_account(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.account_moved() {
            return false;
        }
        self.forget_account(cx);
        true
    }

    /// Nothing read for the previous account stays: the list, the
    /// selection, the open page and its tab, the account's directory and
    /// tags, the tag and the search. Reads in flight stop, and their answers
    /// are dropped; a Back to a place shown before no longer returns there.
    /// The list is read again for the new account while the Space shows,
    /// and otherwise when it is next shown.
    pub(crate) fn forget_account(&mut self, cx: &mut Context<Self>) {
        self.lease = None;
        self.account_epoch = self.account_epoch.wrapping_add(1);
        self.reads = self.reads.wrapping_add(1);
        self.page_reads = self.page_reads.wrapping_add(1);
        std::mem::take(&mut self.stop).stop();
        self._read = None;
        self._page_read = None;
        self._search_task = None;
        self.session = Session::default();
        self.author = None;
        self.history = Default::default();
        self.listing = Listing::default();
        self.page = None;
        self.showing_page = false;
        self.choice.tag = None;
        self.query.clear();
        self.list = ListState::default();
        self.folds = Folds::default();
        // The search field and an open menu need the window: `tidy`.
        self.tidy = true;
        self.sync_lines(cx);
        if self.active {
            self.read(true, cx);
        }
    }

    /// Clears the search field and closes a menu after an account change.
    pub(crate) fn tidy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clear_controls(window, cx) {
            cx.notify();
        }
    }

    /// What `tidy` does, without asking for a frame, so that a render that
    /// finds the account changed draws neither the previous account's
    /// search nor its open menu. True when there was something to clear.
    pub(crate) fn clear_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !std::mem::take(&mut self.tidy) {
            return false;
        }
        self.menu_anchors.dismiss(window, cx);
        self.menu = None;
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        true
    }

    /// Whether what the Space holds was read for an account that is no
    /// longer signed in, before the Space has dropped it.
    pub(crate) fn account_moved(&self) -> bool {
        self.lease.as_ref().is_some_and(Lease::moved)
    }
}
