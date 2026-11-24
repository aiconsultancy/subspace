//! Availability and entry points for the Decisions dialogs.

use gpui_kit::{Context, FocusHandle, Focusable as _, Window};
use subspace_api_client::decision_writes::DecisionReview;
use subspace_api_client::model::{DecisionDetail, DecisionStatus};

use super::{Draft, Editor, Mode, Phase, Target, available};
use crate::Decisions;
use crate::page::PageState;

fn active(detail: &DecisionDetail) -> bool {
    detail
        .project
        .as_ref()
        .is_some_and(|project| !project.archived)
}

// qual:allow(coupling, oi) reason: "Opening a decision dialog checks the current decision, account, focus and Space state."
impl Decisions {
    pub(crate) fn can_author(&self) -> bool {
        available()
            && self.session.account.is_some()
            && self.lease.is_some()
            && self
                .session
                .projects
                .iter()
                .any(|project| project.archived_at.is_none())
    }

    pub(crate) fn can_change(&self, detail: &DecisionDetail) -> bool {
        available()
            && detail.status == DecisionStatus::Proposed
            && active(detail)
            && detail.version.is_some()
            && self.session.account.as_ref().is_some_and(|me| {
                me.kind == "human"
                    || (me.kind == "agent" && me.user_id == detail.recorded_by_user_id)
            })
    }

    pub(crate) fn can_replace(&self, detail: &DecisionDetail) -> bool {
        self.can_author()
            && detail.status == DecisionStatus::Accepted
            && active(detail)
            && detail.superseded_by_id.is_none()
    }

    pub(crate) fn current_detail(&self) -> Option<&DecisionDetail> {
        match &self.shown_page()?.state {
            PageState::Loaded(detail) => Some(detail),
            _ => None,
        }
    }

    fn current_target(&self) -> Option<(&DecisionDetail, Target)> {
        if self.history.snapshot().is_some() {
            return None;
        }
        let detail = self
            .current_detail()
            .filter(|detail| self.can_change(detail))?;
        let target = Target {
            id: detail.id.clone(),
            version: detail.version?,
        };
        Some((detail, target))
    }

    pub(crate) fn new_decision(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.follow_account(cx) || !self.can_author() || self.author.is_some() {
            return;
        }
        let replacing = self
            .current_detail()
            .filter(|detail| replace && self.can_replace(detail));
        if replace && replacing.is_none() {
            return;
        }
        let previous = window.focused(cx);
        if let Some(draft) = Draft::new(self, None, replacing, window, cx) {
            self.open_author(
                Mode::Draft {
                    draft: Box::new(draft),
                    target: None,
                },
                previous,
                window,
                cx,
            );
        }
    }

    pub(crate) fn edit_decision(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.follow_account(cx) || self.author.is_some() {
            return;
        }
        let Some((detail, target)) = self.current_target() else {
            return;
        };
        let previous = window.focused(cx);
        if let Some(draft) = Draft::new(self, Some(detail), None, window, cx) {
            self.open_author(
                Mode::Draft {
                    draft: Box::new(draft),
                    target: Some(target),
                },
                previous,
                window,
                cx,
            );
        }
    }

    pub(crate) fn review_decision(
        &mut self,
        review: DecisionReview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.follow_account(cx) || self.author.is_some() {
            return;
        }
        let Some((detail, target)) = self.current_target() else {
            return;
        };
        let title = detail.title.clone();
        self.open_author(
            Mode::Review {
                target,
                title,
                review,
            },
            window.focused(cx),
            window,
            cx,
        );
    }

    fn open_author(
        &mut self,
        mode: Mode,
        previous_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(lease) = self.lease.clone() else {
            return;
        };
        let focus = cx.focus_handle();
        if matches!(mode, Mode::Review { .. }) {
            focus.focus(window, cx);
        }
        self.author = Some(Editor {
            mode,
            phase: Phase::Ready,
            lease,
            focus,
            previous_focus,
            task: None,
        });
        cx.notify();
    }

    pub(crate) fn close_author(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .author
            .as_ref()
            .is_some_and(|editor| matches!(editor.phase, Phase::Sending))
        {
            return;
        }
        if let Some(editor) = self.author.take() {
            if matches!(editor.phase, Phase::Current(_) | Phase::ReadFailed) {
                self.read_page(cx);
            }
            editor
                .previous_focus
                .unwrap_or_else(|| self.focus_handle(cx))
                .focus(window, cx);
            cx.notify();
        }
    }
}
