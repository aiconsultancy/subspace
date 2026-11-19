//! Read-only side panel, with bounded supersede navigation and focus return.
pub(crate) mod words;

use crate::{Issues, requests::Request};
use gpui_kit::{
    Context, FocusHandle, Point, ScrollHandle, SharedString, Subscription, Window, point, px,
};
use subspace_api_client::{
    hub::{DecisionReadStatus, IssueSnapshot},
    model::DecisionDetail,
};

pub(crate) const CONTEXT: &str = "IssueDecisionPanel";
const STACK_LIMIT: usize = 8;
/// One arrow-key step through the panel body, in pixels.
const SCROLL_LINE: f32 = 40.;
/// The body's scroll position at its top.
const TOP: f32 = 0.;

gpui_kit::actions!(
    issues_decision,
    [
        ScrollUp,
        ScrollDown,
        ScrollPageUp,
        ScrollPageDown,
        ScrollToTop,
        ScrollToBottom
    ]
);

/// A keyboard scroll through the panel body, which has no tab stops of its
/// own when the reasoning has no links.
#[derive(Clone, Copy)]
pub(crate) enum Step {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
}

pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) title: String,
}

pub(crate) struct DecisionPanel {
    pub(crate) stack: Vec<Entry>,
    pub(crate) close: FocusHandle,
    pub(crate) trap: FocusHandle,
    return_to: Option<FocusHandle>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) markdown: Option<[SharedString; 5]>,
    /// Keep focus in the modal panel while it is open.
    containment: Vec<Subscription>,
}

impl DecisionPanel {
    fn new(entry: Entry, return_to: Option<FocusHandle>, cx: &mut Context<Issues>) -> Self {
        Self {
            stack: vec![entry],
            close: cx.focus_handle(),
            trap: cx.focus_handle(),
            return_to,
            scroll: ScrollHandle::new(),
            markdown: None,
            containment: Vec::new(),
        }
    }

    fn scroll_by(&self, step: Step) {
        let viewport = f32::from(self.scroll.bounds().size.height);
        let max = f32::from(self.scroll.max_offset().y);
        let offset = self.scroll.offset();
        let current = -f32::from(offset.y);
        let page = (viewport - SCROLL_LINE).max(SCROLL_LINE);
        let target = match step {
            Step::Up => current - SCROLL_LINE,
            Step::Down => current + SCROLL_LINE,
            Step::PageUp => current - page,
            Step::PageDown => current + page,
            Step::Top => TOP,
            Step::Bottom => max,
        };
        self.scroll
            .set_offset(point(offset.x, px(-target.clamp(TOP, max.max(TOP)))));
    }

    fn select(&mut self, entry: Entry) {
        if self.stack.len() == STACK_LIMIT {
            self.stack.remove(0);
        }
        self.stack.push(entry);
    }

    fn follow(&mut self, issue: &IssueSnapshot) {
        if self.markdown.is_none()
            && let Some(entry) = self.stack.last_mut()
            && let Some(DecisionReadStatus::Loaded(detail)) = issue.decision(&entry.id)
        {
            entry.title.clone_from(&detail.title);
            self.markdown = Some(prepare_markdown(detail));
        }
    }

    fn prepare_read(&mut self) -> Option<(String, FocusHandle)> {
        let id = self.stack.last()?.id.clone();
        self.scroll.set_offset(Point::default());
        self.markdown = None;
        Some((id, self.close.clone()))
    }
}

fn prepare_markdown(detail: &DecisionDetail) -> [SharedString; 5] {
    [
        &detail.decision,
        &detail.context,
        &detail.why,
        &detail.implications,
        &detail.alternatives,
    ]
    .map(|source| {
        crate::markdown::prepare_markdown(source.trim(), &crate::markdown::Options::new(&[], &[]))
            .into()
    })
}

pub(crate) fn follow(panel: &mut Option<DecisionPanel>, issue: &IssueSnapshot) {
    if issue.detail().is_none() {
        *panel = None;
    } else if let Some(panel) = panel {
        panel.follow(issue);
    }
}

pub(crate) fn open(
    issues: &mut Issues,
    id: &str,
    title: &str,
    window: &mut Window,
    cx: &mut Context<Issues>,
) {
    let entry = Entry {
        id: id.into(),
        title: title.into(),
    };
    let return_to = window.focused(cx);
    issues.close_text(window, cx);
    let mut panel = DecisionPanel::new(entry, return_to, cx);
    panel.containment = vec![
        cx.on_focus_out(&panel.trap, window, |this, _, window, cx| {
            contain(this, window, cx);
        }),
        cx.on_focus_in(&issues.focus, window, contain),
    ];
    issues.decision = Some(panel);
    read(issues, window, cx);
}

/// The issue page behind the panel stays visible but inert: a control there
/// that takes focus some other way than Tab, such as an assistive
/// technology's focus action, hands it back to the panel. Focus that leaves
/// the board, for the command palette say, is left alone, as is the
/// attachment panel that replaces this one.
fn contain(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    let Some(panel) = &issues.decision else {
        return;
    };
    if !issues.media.text_open()
        && issues.focus.contains_focused(window, cx)
        && !panel.trap.contains_focused(window, cx)
    {
        panel.close.focus(window, cx);
    }
}

pub(crate) fn scroll(issues: &mut Issues, step: Step, cx: &mut Context<Issues>) {
    if let Some(panel) = &issues.decision {
        panel.scroll_by(step);
        cx.notify();
    }
}

pub(crate) fn close(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) -> bool {
    let Some(panel) = issues.decision.take() else {
        return false;
    };
    let target = panel
        .return_to
        .filter(|handle| issues.focus.contains(handle, window));
    target
        .unwrap_or_else(|| issues.focus.clone())
        .focus(window, cx);
    issues.request(Request::CloseDecision, cx);
    cx.notify();
    true
}
pub(crate) fn follow_link(
    issues: &mut Issues,
    entry: Entry,
    window: &mut Window,
    cx: &mut Context<Issues>,
) {
    if let Some(panel) = &mut issues.decision {
        panel.select(entry);
        read(issues, window, cx);
    }
}

pub(crate) fn back(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    if let Some(panel) = &mut issues.decision
        && panel.stack.len() > 1
    {
        panel.stack.pop();
        read(issues, window, cx);
    }
}

pub(crate) fn read(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    request(issues, Request::OpenDecision, window, cx);
}

pub(crate) fn retry(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) {
    request(issues, Request::RetryDecision, window, cx);
}

fn request(
    issues: &mut Issues,
    request: impl FnOnce(String) -> Request,
    window: &mut Window,
    cx: &mut Context<Issues>,
) {
    let Some((id, close)) = issues
        .decision
        .as_mut()
        .and_then(DecisionPanel::prepare_read)
    else {
        return;
    };
    // The Close control tracks this handle after the next draw.
    close.focus(window, cx);
    cx.defer_in(window, move |this, window, cx| {
        if this
            .decision
            .as_ref()
            .is_some_and(|panel| panel.close == close)
        {
            close.focus(window, cx);
        }
    });
    issues.request(request(id), cx);
    cx.notify();
}
