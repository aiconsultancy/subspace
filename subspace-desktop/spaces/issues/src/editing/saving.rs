use gpui_kit::{App, AppContext as _, Context, Window};
use subspace_api_client::{
    hub::{Hub, IssueWriteScope},
    issue_writes::{CreateKey, IssueChanges, IssueSave, IssueSaveError},
};

use super::{Field, IssueEditor, Phase, confirmation::ChildChanges};
use crate::Issues;

enum Target {
    Create { project: String, key: CreateKey },
    Update { issue: String, version: u32 },
}

struct Save {
    scope: IssueWriteScope,
    target: Target,
    changes: IssueChanges,
}

impl Save {
    fn from_editor(editor: &IssueEditor, cx: &App) -> Result<Self, IssueSaveError> {
        let changes = editor.changes(cx);
        changes.validate(!editor.editing_existing())?;
        let target = match editor.basis.as_ref() {
            Some(issue) => Target::Update {
                issue: issue.fields.id.clone(),
                version: issue
                    .fields
                    .version
                    .try_into()
                    .map_err(|_| IssueSaveError::Conflict)?,
            },
            None => Target::Create {
                project: editor
                    .properties
                    .id(Field::Project)
                    .ok_or(IssueSaveError::Unavailable)?
                    .to_owned(),
                key: editor.key,
            },
        };
        Ok(Self {
            scope: editor.scope.clone(),
            target,
            changes,
        })
    }

    fn send(self, hub: &Hub) -> Result<IssueSave, IssueSaveError> {
        match &self.target {
            Target::Create { project, key } => {
                hub.create_issue(&self.scope, project, key, &self.changes)
            }
            Target::Update { issue, version } => {
                hub.update_issue(&self.scope, issue, *version, &self.changes)
            }
        }
    }
}

// qual:allow(coupling, oi) reason: "The editor dispatches scoped writes and applies their outcomes to the board."
impl Issues {
    pub(super) fn check_issue_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if editor.busy() {
            return;
        }
        if editor.editing_existing() {
            return self.read_issue_conflict(window, cx);
        }
        let Some(hub) = self.hub.clone() else {
            return;
        };
        let scope = editor.scope.clone();
        let key = editor.key;
        editor.phase = Phase::Checking;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { hub.check_created_issue(&scope, &key) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some(editor) = this.current_editor(key.value()) {
                    match result {
                        Ok(Some(issue)) => this.finish_editor_save(
                            key.value(),
                            Ok(IssueSave::Saved(issue)),
                            window,
                            cx,
                        ),
                        Ok(None) => editor.phase = Phase::Checked,
                        Err(IssueSaveError::SeveralMatches) => editor.phase = Phase::SeveralMatches,
                        Err(_) => editor.phase = Phase::CheckFailed,
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn submit_issue_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_none() {
            self.save_issue_editor(window, cx);
        }
    }

    pub(super) fn save_issue_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_with_children(ChildChanges::default(), window, cx);
    }

    pub(super) fn save_with_children(
        &mut self,
        children: ChildChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(editor), Some(hub)) = (self.editor.as_mut(), self.hub.clone()) else {
            return;
        };
        if editor.busy()
            || editor.discard
            || matches!(
                editor.phase,
                Phase::Conflict(_)
                    | Phase::ConfirmingChildren(_)
                    | Phase::Unconfirmed
                    | Phase::CheckFailed
                    | Phase::SeveralMatches
            )
        {
            return;
        }
        editor.summary_validation = true;
        let mut task = match Save::from_editor(editor, cx) {
            Ok(task) => task,
            Err(error) => {
                editor.phase = Phase::Failed(error);
                cx.notify();
                return;
            }
        };
        if let Some(confirmation) = children.confirmation(editor) {
            editor.phase = Phase::ConfirmingChildren(confirmation);
            editor.reveal_save_notice();
            cx.notify();
            return;
        }
        children.install(&mut task.changes);
        let key = editor.key.value();
        editor.phase = Phase::Sending;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { task.send(&hub) }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_editor_save(key, result, window, cx)
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_editor_save(
        &mut self,
        key: u64,
        result: Result<IssueSave, IssueSaveError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.current_editor(key) else {
            return;
        };
        match result {
            Ok(IssueSave::Saved(issue)) => {
                let edit = editor.editing_existing();
                let saved_parent =
                    issue.fields.parent_id.as_ref().filter(|id| {
                        editor.subissue_of.as_ref().map(|parent| &parent.id) == Some(*id)
                    });
                let keep_parent = saved_parent.is_some_and(|id| {
                    self.issue_view
                        .snapshot
                        .as_ref()
                        .and_then(|open| open.detail())
                        .is_some_and(|open| &open.fields.id == id)
                });
                self.close_issue_editor(window, cx);
                self.reload_list(cx);
                if keep_parent {
                    self.issue_view.project_subissue(issue.fields);
                    self.reload_detail(cx);
                } else if edit {
                    self.reload_detail(cx);
                } else {
                    self.open_detail(issue.fields.identifier.clone(), cx);
                }
            }
            Err(IssueSaveError::Conflict) if editor.editing_existing() => {
                editor.phase = Phase::Conflict(None)
            }
            Err(IssueSaveError::Unconfirmed) => editor.phase = Phase::Unconfirmed,
            Err(
                error @ (IssueSaveError::TrackerChildrenBlocked | IssueSaveError::ParentRefused),
            ) => {
                editor.phase = Phase::Failed(error);
                editor.reveal_save_notice();
            }
            Err(error) => editor.phase = Phase::Failed(error),
        }
        cx.notify();
    }

    pub(super) fn current_editor(&mut self, key: u64) -> Option<&mut IssueEditor> {
        let scope = self.hub.as_ref()?.issue_write_scope()?;
        self.editor
            .as_mut()
            .filter(|editor| editor.key.value() == key && editor.scope == scope)
    }

    pub(super) fn read_issue_conflict(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(editor), Some(hub)) = (self.editor.as_mut(), self.hub.clone()) else {
            return;
        };
        let Some(issue) = editor.basis.as_ref() else {
            return;
        };
        let id = issue.fields.id.clone();
        let key = editor.key.value();
        let scope = editor.scope.clone();
        editor.phase = Phase::LoadingConflict;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { hub.latest_issue(&scope, &id).map(Box::new) })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if let Some(editor) = this.current_editor(key) {
                    editor.phase = match result {
                        Ok(issue) if issue.fields.archived_at.is_some() => {
                            Phase::Failed(IssueSaveError::IssueArchived)
                        }
                        Ok(issue) => Phase::Conflict(Some(issue)),
                        Err(_) => Phase::Conflict(None),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn keep_issue_edits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Phase::Conflict(Some(issue)) = std::mem::take(&mut editor.phase) else {
            return;
        };
        editor.keep_edits(&self.projects, issue, window, cx);
        cx.notify();
    }

    /// Rebases the draft on the version changed elsewhere, keeping the
    /// fields the person changed.
    pub(super) fn reload_changed_issue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if editor.frozen() || matches!(editor.phase, Phase::Conflict(_)) {
            return;
        }
        let Some(issue) = editor.elsewhere.take() else {
            return;
        };
        editor.keep_edits(&self.projects, issue, window, cx);
        cx.notify();
    }
}

impl IssueEditor {
    fn reveal_save_notice(&self) {
        // The full editor's notice is above the scrollable properties. A
        // combined edit can leave the viewport down at the Parent picker.
        if self.inline.is_none() {
            self.scroll.set_offset(Default::default());
        }
    }
}
