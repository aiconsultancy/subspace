//! Where the issue page's Back goes: the issues list, or the view the issue
//! was opened from in another Space.
//!
//! The shell tells the board which view the person was on each time it
//! makes the board current ([`Dashboard::arrived_from`]). An issue opened by
//! a link from there, such as an issue chip in a channel, returns to it. An
//! issue opened from the list or the activity feed returns to the list, with
//! its filters and selection as they were. Opening another issue from the
//! page, such as a parent, sub-issue or relation, keeps the first issue's
//! origin, so Back leaves the issues rather than stepping through them.
//!
//! [`Dashboard::arrived_from`]: subspace_framework::dashboard::Dashboard::arrived_from

use gpui_kit::{Context, SharedString};
use subspace_framework::dashboard::{DashboardEvent, ReturnPoint};

use crate::Issues;

/// The label Back shows when it returns to the list.
pub(crate) const TO_LIST: &str = "Back to issues";

#[derive(Debug, Default)]
pub(crate) struct Back {
    /// The view the shell last said the board was reached from, until a
    /// link opens an issue from it.
    arrival: Option<ReturnPoint>,
    /// Where the open issue's Back goes; `None` is the issues list.
    origin: Option<ReturnPoint>,
}

impl Back {
    pub(crate) fn arrived(&mut self, from: Option<ReturnPoint>) {
        self.arrival = from;
    }

    /// An issue page opens with no page open before: Back goes to the list
    /// until a link says otherwise.
    pub(crate) fn opened_from_list(&mut self) {
        self.origin = None;
    }

    /// The issue opening now came by link. From another view, the one the
    /// shell said the board was reached from, Back returns there, even
    /// over an issue page left open earlier. From this board, as with the
    /// palette, the open page's origin stays.
    pub(crate) fn opened_by_link(&mut self) {
        if let Some(arrival) = self.arrival.take() {
            self.origin = Some(arrival);
        }
    }

    /// The page closed: whatever opens next starts again.
    pub(crate) fn closed(&mut self) {
        self.origin = None;
    }

    pub(crate) fn origin(&self) -> Option<&ReturnPoint> {
        self.origin.as_ref()
    }

    /// The Back control's text, naming its destination.
    pub(crate) fn label(&self) -> SharedString {
        self.origin
            .as_ref()
            .map_or_else(|| TO_LIST.into(), ReturnPoint::back_label)
    }
}

// qual:allow(coupling, oi) reason: "Back's origin and its event stay beside the Back state; the page and list keep their own modules."
impl Issues {
    /// Back from the issue page: to the list, then on to the view the issue
    /// was opened from, if any. The list shows first, so when that view no
    /// longer exists, as a channel left since, the list is where Back lands.
    pub(crate) fn go_back(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        if self.inline_navigation_blocked(cx) {
            return;
        }
        let origin = self.back.origin().cloned();
        self.back_to_list(window, cx);
        if let Some(origin) = origin {
            cx.emit(DashboardEvent::Return(origin));
        }
    }
}
