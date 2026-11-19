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
}

impl TimelineMode {
    /// The mode's position in the Comments and All tab bar.
    pub(crate) fn tab_index(self) -> usize {
        usize::from(self == Self::All)
    }

    /// The mode at `index` in that tab bar.
    pub(crate) fn at(index: usize) -> Self {
        if index == 0 {
            Self::Comments
        } else {
            Self::All
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
        }
    }

    pub(crate) fn read_goal(&self) -> usize {
        match self.mode {
            TimelineMode::Comments => self.limit(),
            TimelineMode::All => 0,
        }
    }

    pub(crate) fn filtered<'a>(
        &self,
        items: &'a [IssueActivity],
    ) -> impl Iterator<Item = &'a IssueActivity> {
        let mode = self.mode;
        items.iter().filter(move |entry| {
            mode == TimelineMode::All || entry.activity.kind() == ActivityKind::CommentCreated
        })
    }
}

pub(crate) struct TimelineFocus {
    pub(crate) handle: FocusHandle,
    pub(crate) anchor: ScrollAnchor,
}

impl TimelineFocus {
    pub(crate) fn new(scroll: &ScrollHandle, cx: &Context<Issues>) -> Self {
        Self {
            handle: cx.focus_handle(),
            anchor: ScrollAnchor::for_handle(scroll.clone()),
        }
    }

    fn reveal(&self, window: &mut Window, cx: &mut Context<Issues>) {
        self.handle.focus(window, cx);
        self.anchor.scroll_to(window, cx);
    }
}

// qual:allow(coupling, oi) reason: "Timeline actions route board input to the existing read-only issue queue."
impl Issues {
    pub(crate) fn start_timeline(&mut self, issue: &IssueSnapshot, cx: &mut Context<Self>) {
        if *issue.status() == IssueStatus::Loaded && issue.timeline().idle() {
            self.request(
                Request::OpenTimeline(self.issue_view.timeline.read_goal()),
                cx,
            );
        }
    }

    pub(crate) fn show_timeline(&mut self, mode: TimelineMode, cx: &mut Context<Self>) {
        self.issue_view.timeline.mode = mode;
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
    fn the_comments_and_all_tabs_map_to_their_modes() {
        for mode in [TimelineMode::Comments, TimelineMode::All] {
            assert_eq!(TimelineMode::at(mode.tab_index()), mode);
        }
        assert_eq!(TimelineMode::Comments.tab_index(), 0);
        assert_eq!(TimelineMode::at(1), TimelineMode::All);
    }
}
