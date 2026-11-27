use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{Context, FocusHandle, ScrollAnchor, ScrollHandle, Window};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus};
use subspace_api_client::model::{ActivityKind, IssueActivity};

use crate::Issues;
use crate::requests::Request;

const COMMENTS_PER_STEP: usize = 6;
pub(crate) mod entry;
const EVENTS_PER_STEP: usize = 50;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TimelineMode {
    #[default]
    Comments,
    All,
    Decisions,
}

impl TimelineMode {
    /// The mode's position in the Activity tab bar.
    pub(crate) fn tab_index(self) -> usize {
        match self {
            Self::Comments => 0,
            Self::All => 1,
            Self::Decisions => 2,
        }
    }

    /// The mode at `index` in that tab bar.
    pub(crate) fn at(index: usize) -> Self {
        match index {
            1 => Self::All,
            2 => Self::Decisions,
            _ => Self::Comments,
        }
    }
}

#[derive(Default)]
pub(crate) struct TimelineView {
    pub(crate) mode: TimelineMode,
    shown_comments: usize,
    shown_events: usize,
}

impl TimelineView {
    pub(crate) fn show_all_loaded(&mut self, items: &[IssueActivity]) {
        self.mode = TimelineMode::Comments;
        self.shown_comments = self
            .filtered(items)
            .count()
            .saturating_sub(COMMENTS_PER_STEP);
    }
    pub(crate) fn limit(&self) -> usize {
        match self.mode {
            TimelineMode::Comments => self.shown_comments.saturating_add(COMMENTS_PER_STEP),
            TimelineMode::All => self.shown_events.saturating_add(EVENTS_PER_STEP),
            TimelineMode::Decisions => 0,
        }
    }

    pub(crate) fn read_goal(&self) -> usize {
        match self.mode {
            TimelineMode::Comments => self.limit(),
            TimelineMode::All | TimelineMode::Decisions => 0,
        }
    }

    pub(crate) fn filtered<'a>(
        &self,
        items: &'a [IssueActivity],
    ) -> impl Iterator<Item = &'a IssueActivity> {
        let mode = self.mode;
        items.iter().filter(move |entry| match mode {
            TimelineMode::Comments => entry.activity.kind() == ActivityKind::CommentCreated,
            TimelineMode::All => true,
            TimelineMode::Decisions => false,
        })
    }
}

/// GPUI Kit's tab list owns each tab's focus handle and makes the selected
/// tab its one tab stop. `tabs` is a handle on the list's wrapper, not a tab
/// stop itself, so focusing it and moving to the next stop reaches the
/// selected tab.
pub(crate) struct TimelineFocus {
    pub(crate) tabs: FocusHandle,
    pub(crate) anchor: ScrollAnchor,
    /// The latest focus request: an earlier one still waiting does nothing.
    request: Rc<Cell<u64>>,
}

/// Frames a focus request waits: the first draws the new selection and
/// GPUI Kit's tab list marks the selected tab its Tab stop while laying it
/// out; the second draws the tab with that flag, so the window's Tab order
/// has it; focus moves before the third.
const FRAMES_UNTIL_TAB_STOP: usize = 2;

impl TimelineFocus {
    pub(crate) fn new(scroll: &ScrollHandle, cx: &Context<Issues>) -> Self {
        Self {
            tabs: cx.focus_handle().tab_stop(false),
            anchor: ScrollAnchor::for_handle(scroll.clone()),
            request: Rc::default(),
        }
    }

    /// Focuses the selected Activity tab once the window's Tab order has it
    /// as the list's one Tab stop. Nothing is focused while the list is not
    /// drawn, so focus never falls to the window's first Tab stop.
    pub(crate) fn focus_selected(&self, window: &mut Window, cx: &mut Context<Issues>) {
        let request = self.request.get() + 1;
        self.request.set(request);
        let (tabs, latest) = (self.tabs.clone(), self.request.clone());
        after_frames(FRAMES_UNTIL_TAB_STOP, window, move |window, cx| {
            if latest.get() == request {
                focus_tab_stop(&tabs, window, cx);
            }
        });
        cx.notify();
    }

    fn reveal(&self, window: &mut Window, cx: &mut Context<Issues>) {
        self.focus_selected(window, cx);
        self.anchor.scroll_to(window, cx);
    }
}

/// Runs `then` before the frame after `frames` more drawn frames, each
/// drawn whether or not anything else changed.
fn after_frames(
    frames: usize,
    window: &mut Window,
    then: impl FnOnce(&mut Window, &mut gpui_kit::App) + 'static,
) {
    window.on_next_frame(move |window, cx| {
        if frames == 0 {
            then(window, cx);
        } else {
            window.refresh();
            after_frames(frames - 1, window, then);
        }
    });
}

/// Moves focus from the list's wrapper to its one Tab stop, the selected
/// tab, if the list was drawn.
fn focus_tab_stop(tabs: &FocusHandle, window: &mut Window, cx: &mut gpui_kit::App) {
    if tabs.is_rendered(window) {
        tabs.focus(window, cx);
        window.focus_next(cx);
    }
}

// qual:allow(coupling, oi) reason: "Timeline actions route board input to the existing read-only issue queue."
impl Issues {
    pub(crate) fn start_timeline(&mut self, issue: &IssueSnapshot, cx: &mut Context<Self>) {
        if self.issue_view.timeline.mode == TimelineMode::Decisions {
            if issue.detail().is_some()
                && issue.decisions().is_none()
                && !issue.decisions_loading()
                && issue.decisions_failed().is_none()
                && !issue.decisions_unavailable()
            {
                self.request(Request::OpenDecisions, cx);
            }
        } else if *issue.status() == IssueStatus::Loaded && issue.timeline().idle() {
            self.request(
                Request::OpenTimeline(self.issue_view.timeline.read_goal()),
                cx,
            );
        }
    }

    pub(crate) fn show_timeline(&mut self, mode: TimelineMode, cx: &mut Context<Self>) {
        self.issue_view.timeline.mode = mode;
        if let Some(issue) = self.issue_view.snapshot.clone() {
            self.start_timeline(&issue, cx);
        }
        cx.notify();
    }

    pub(crate) fn more_timeline(&mut self, cx: &mut Context<Self>) {
        let Some(issue) = self.issue_view.snapshot.as_ref() else {
            return;
        };
        let Some(page) = issue.timeline().page() else {
            return;
        };
        let timeline = &mut self.issue_view.timeline;
        let loaded = timeline.filtered(&page.items).count();
        match timeline.mode {
            TimelineMode::Comments => {
                timeline.shown_comments = timeline
                    .limit()
                    .max(loaded.saturating_sub(COMMENTS_PER_STEP));
            }
            TimelineMode::All => timeline.shown_events += EVENTS_PER_STEP,
            TimelineMode::Decisions => return,
        }
        if loaded < timeline.limit() && page.next_cursor.is_some() {
            self.request(
                Request::MoreTimeline(self.issue_view.timeline.read_goal()),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn retry_timeline(&mut self, cx: &mut Context<Self>) {
        self.request(
            Request::RetryTimeline(self.issue_view.timeline.read_goal()),
            cx,
        );
    }

    pub(crate) fn focus_timeline(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = &self.issue_view.timeline_focus {
            focus.reveal(window, cx);
        }
    }
}

#[cfg(test)]
mod mode_tests {
    use super::TimelineMode;

    #[test]
    fn the_activity_tabs_map_to_their_modes() {
        for mode in [
            TimelineMode::Comments,
            TimelineMode::All,
            TimelineMode::Decisions,
        ] {
            assert_eq!(TimelineMode::at(mode.tab_index()), mode);
        }
        assert_eq!(TimelineMode::Comments.tab_index(), 0);
        assert_eq!(TimelineMode::at(1), TimelineMode::All);
        assert_eq!(TimelineMode::at(2), TimelineMode::Decisions);
    }
}
