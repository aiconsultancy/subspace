//! Read-only side panel, with bounded supersede navigation and focus return.
pub(crate) mod words;

use crate::{Issues, requests::Request};
use gpui_kit::{Context, FocusHandle, Point, ScrollHandle, SharedString, Window};
use subspace_api_client::{
    hub::{DecisionReadStatus, IssueSnapshot},
    model::DecisionDetail,
};

pub(crate) const CONTEXT: &str = "IssueDecisionPanel";
const STACK_LIMIT: usize = 8;

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
        }
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
    issues.decision = Some(DecisionPanel::new(entry, return_to, cx));
    read(issues, window, cx);
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
