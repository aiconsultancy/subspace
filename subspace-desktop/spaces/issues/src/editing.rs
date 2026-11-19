//! Retained, in-memory create and edit drafts for the Subspace API connection.

mod draft;
mod entry;
mod properties;
mod saving;
mod state_change;
pub(crate) use entry::EditTarget;
pub(crate) use properties::Field;
mod target;
mod text;
pub(crate) mod view;

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{App, Context, Entity, FocusHandle, Subscription, Window};
use std::collections::HashMap;
use subspace_api_client::{
    hub::{HubSnapshot, IssueWriteScope},
    issue_writes::{ChoiceSource, CreateKey, IssueChoice, IssueSaveError},
    model::IssueDetail,
};

use crate::Issues;
use properties::Properties;
use target::editable_detail;
use text::{TextValues, input, input_changed};

const DESCRIPTION_MIN_ROWS: usize = 4;
const DESCRIPTION_MAX_ROWS: usize = 10;

pub(crate) struct IssueEditor {
    inline: Option<EditTarget>,
    scope: IssueWriteScope,
    key: CreateKey,
    basis: Option<Box<IssueDetail>>,
    properties: Properties,
    catalogues: HashMap<ChoiceSource, Vec<IssueChoice>>,
    title: Entity<InputState>,
    summary: Entity<InputState>,
    summary_validation: bool,
    description: Entity<TextareaState>,
    due: Entity<InputState>,
    parent: Entity<InputState>,
    phase: Phase,
    /// A newer version of the edited issue, read after a change elsewhere.
    elsewhere: Option<Box<IssueDetail>>,
    discard: bool,
    previous_focus: Option<FocusHandle>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
enum Phase {
    #[default]
    Editing,
    Sending,
    LoadingConflict,
    Conflict(Option<Box<IssueDetail>>),
    Unconfirmed,
    Checked,
    Checking,
    CheckFailed,
    SeveralMatches,
    Failed(IssueSaveError),
}

// qual:allow(coupling, oi) reason: "Editor entry points combine the retained board with input focus and the current hub scope."
impl Issues {
    pub(crate) fn can_create_issue(&self) -> bool {
        self.is_ready()
            && !crate::initiative::no_initiative_projects(self)
            && self
                .hub
                .as_ref()
                .and_then(|hub| hub.issue_write_scope())
                .is_some()
            && self
                .projects
                .iter()
                .any(|project| project.archived_at.is_none())
    }

    pub(crate) fn can_edit_issue(&self) -> bool {
        self.is_ready()
            && self
                .hub
                .as_ref()
                .and_then(|hub| hub.issue_write_scope())
                .is_some()
            && editable_detail(self).is_some()
    }

    pub(crate) fn open_issue_editor(
        &mut self,
        edit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.is_none()
            && self.project_move.is_none()
            && let Some(opening) = IssueEditor::opening(self, edit)
        {
            self.editor = Some(IssueEditor::new(self, opening, window, cx));
            cx.notify();
        }
    }

    pub(crate) fn follow_editor(&mut self, snapshot: &HubSnapshot, cx: &mut Context<Self>) {
        if self
            .editor
            .as_ref()
            .is_some_and(|editor| snapshot.issue_write_scope().as_ref() != Some(&editor.scope))
        {
            self.editor = None;
            self.menu = None;
            cx.notify();
        }
        if self
            .project_move
            .as_ref()
            .is_some_and(|m| snapshot.issue_write_scope().as_ref() != Some(&m.scope))
        {
            self.project_move = None;
            self.menu = None;
            cx.notify();
        }
        if let Some(editor) = &mut self.editor {
            editor.follow_milestone(snapshot);
            if editor.follow_elsewhere(snapshot) {
                cx.notify();
            }
        }
    }

    pub(crate) fn inline_field_active(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.inline.is_some())
            || self.project_move.as_ref().is_some_and(|m| !m.confirming)
    }

    pub(crate) fn full_editor_active(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.inline.is_none())
    }

    /// Whether the edited issue changed elsewhere since the draft's basis.
    #[cfg(test)]
    pub(crate) fn issue_changed_elsewhere(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.elsewhere.is_some())
    }

    pub(crate) fn issue_editor_busy(&self) -> bool {
        self.editor.as_ref().is_some_and(IssueEditor::busy)
    }

    pub(crate) fn issue_editor_dirty(&self, cx: &App) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.dirty(cx) || editor.busy())
    }

    pub(crate) fn restore_editor_issue(&mut self, cx: &mut Context<Self>) {
        let target = self
            .editor
            .as_ref()
            .and_then(|editor| editor.basis.as_ref())
            .map(|issue| issue.fields.identifier.clone())
            .or_else(|| self.project_move.as_ref().map(|m| m.id.clone()));
        if let Some(identifier) = target
            && self.issue_view.snapshot.is_none()
        {
            self.open_detail(identifier, cx);
        }
    }
    fn close_issue_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.take() {
            if let Some(hub) = &self.hub {
                hub.forget_issue_create(&editor.key);
            }
            let focus = editor.previous_focus.unwrap_or_else(|| self.focus.clone());
            focus.focus(window, cx);
            cx.notify();
        }
    }

    fn cancel_issue_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.busy() {
            return;
        }
        if !editor.dirty(cx) {
            self.close_issue_editor(window, cx);
        } else {
            editor.discard = true;
            cx.notify();
        }
    }

    fn editor_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.editor {
            if matches!(
                editor.phase,
                Phase::Failed(
                    IssueSaveError::Summary
                        | IssueSaveError::Title
                        | IssueSaveError::Description
                        | IssueSaveError::DueDate
                        | IssueSaveError::Invalid
                        | IssueSaveError::NoChanges
                )
            ) {
                editor.phase = Phase::Editing;
            }
            cx.notify();
        }
    }
}
