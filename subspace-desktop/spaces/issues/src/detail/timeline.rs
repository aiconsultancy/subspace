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

    pub(crate) fn adjacent(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Comments, true) | (Self::Decisions, false) => Self::All,
            (Self::All, true) | (Self::Comments, false) => Self::Decisions,
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

pub(crate) struct TimelineFocus {
    tabs: [FocusHandle; Self::TAB_COUNT],
    pub(crate) anchor: ScrollAnchor,
}

impl TimelineFocus {
    const TAB_COUNT: usize = 3;

    pub(crate) fn new(scroll: &ScrollHandle, cx: &Context<Issues>) -> Self {
        Self {
            tabs: std::array::from_fn(|_| cx.focus_handle()),
            anchor: ScrollAnchor::for_handle(scroll.clone()),
        }
    }

    pub(crate) fn tab(&self, mode: TimelineMode) -> &FocusHandle {
        &self.tabs[mode.tab_index()]
    }

    fn reveal(&self, mode: TimelineMode, window: &mut Window, cx: &mut Context<Issues>) {
        self.tab(mode).focus(window, cx);
        self.anchor.scroll_to(window, cx);
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
            focus.reveal(self.issue_view.timeline.mode, window, cx);
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
