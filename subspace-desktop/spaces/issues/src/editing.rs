//! Retained, in-memory create and edit drafts for the Subspace API connection.

mod confirmation;
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

use gpui_kit::component::input::InputState;
use gpui_kit::{App, Context, Entity, FocusHandle, ScrollHandle, Subscription, Window};
use std::collections::HashMap;
use subspace_api_client::{
    hub::{HubSnapshot, IssueWriteScope},
    issue_writes::{ChoiceSource, CreateKey, IssueChoice, IssueSaveError},
    model::IssueDetail,
};

use crate::Issues;
use confirmation::{ChildChanges, TrackerConfirmation};
use properties::Properties;
use target::{NewSubissue, editable_detail};
use text::{TextValues, input, input_changed};

use subspace_framework::dashboard::markdown_editor::MarkdownEditor;

pub(crate) struct IssueEditor {
    inline: Option<EditTarget>,
    scope: IssueWriteScope,
    key: CreateKey,
    basis: Option<Box<IssueDetail>>,
    initial_text: TextValues,
    properties: Properties,
    catalogues: HashMap<ChoiceSource, Vec<IssueChoice>>,
    title: Entity<InputState>,
    summary: Entity<InputState>,
    summary_validation: bool,
    description: Entity<MarkdownEditor>,
    scroll: ScrollHandle,
    due: Entity<InputState>,
    parent: Entity<InputState>,
    subissue_of: Option<NewSubissue>,
    parent_search: Option<view::ParentSearch>,
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
    ConfirmingChildren(TrackerConfirmation),
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
        if !self.editor_slot_available() {
            return;
        }
        let Some(opening) = IssueEditor::opening(self, edit) else {
            return;
        };
        self.install_editor(opening, window, cx);
    }

    pub(crate) fn can_create_subissue(&self, detail: &IssueDetail) -> bool {
        view::searches_trackers()
            && detail.children.is_some()
            && self.can_create_issue()
            && editable_detail(self).is_some_and(|open| open.fields.id == detail.fields.id)
    }

    pub(crate) fn new_subissue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(opening) = self.subissue_opening() else {
            return;
        };
        self.install_editor(opening, window, cx);
    }

    fn editor_slot_available(&self) -> bool {
        self.editor.is_none() && self.project_move.is_none()
    }

    fn subissue_opening(&self) -> Option<draft::Opening> {
        if !self.editor_slot_available() {
            return None;
        }
        let detail = editable_detail(self).filter(|issue| self.can_create_subissue(issue))?;
        IssueEditor::opening_subissue(self, detail)
    }

    fn install_editor(
        &mut self,
        opening: draft::Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor = Some(IssueEditor::new(self, opening, window, cx));
        cx.notify();
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
            self.menu_anchors.dismiss(window, cx);
            self.menu = None;
            if let Some(hub) = &self.hub {
                hub.forget_issue_create(&editor.key);
            }
            let focus = editor.previous_focus.unwrap_or_else(|| self.focus.clone());
            focus.focus(window, cx);
            cx.notify();
        }
    }

    fn answer_tracker_confirmation(
        &mut self,
        yes: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Phase::ConfirmingChildren(confirmation) = editor.phase else {
            return;
        };
        editor.phase = Phase::Editing;
        let changes = match confirmation {
            TrackerConfirmation::Assignment => ChildChanges {
                assign_children: Some(yes),
                move_children: false,
            },
            TrackerConfirmation::State { assign_children } if yes => ChildChanges {
                assign_children,
                move_children: true,
            },
            TrackerConfirmation::State { .. } => {
                cx.notify();
                return;
            }
        };
        self.save_with_children(changes, window, cx);
    }

    fn cancel_issue_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.busy() {
            return;
        }
        if matches!(editor.phase, Phase::ConfirmingChildren(_)) {
            editor.phase = Phase::Editing;
            cx.notify();
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
