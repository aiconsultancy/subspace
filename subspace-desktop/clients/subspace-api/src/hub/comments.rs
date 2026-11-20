//! Memory-only drafts. Ambiguous writes stay frozen until a complete thread check.
mod checking;
mod images;
pub(super) use images::next_attachment_id;
pub use images::{Attachment, AttachmentState, ImageIntake};
mod draft;
mod reading;
mod sending;
use super::{Hub, HubError, HubStatus, State, lock};
pub(super) use checking::check;
pub use draft::{CommentDraft, CommentState};
use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct DraftKey {
    pub(in crate::hub) issue: String,
    parent: Option<String>,
}
pub(super) type Drafts = HashMap<DraftKey, CommentDraft>;
impl DraftKey {
    fn new(issue: &str, parent: Option<&str>) -> Self {
        Self {
            issue: issue.into(),
            parent: parent.map(str::to_owned),
        }
    }
}

impl State {
    fn writing_reason(&self, issue_id: &str, parent: Option<&str>) -> Option<&'static str> {
        if self.snapshot.status != HubStatus::Ready {
            return Some("Sign in under Settings › Account, then reload Issues.");
        }
        let Some(issue) = self
            .snapshot
            .issue
            .as_ref()
            .and_then(|issue| issue.detail.as_ref())
            .filter(|issue| issue.fields.id == issue_id || issue.fields.identifier == issue_id)
        else {
            return Some("Open the issue before posting a comment.");
        };
        if issue.fields.archived_at.is_some()
            || self.snapshot.projects.iter().any(|project| {
                project.id == issue.fields.project_id && project.archived_at.is_some()
            })
        {
            return Some("This issue or its project is archived.");
        }
        parent.and_then(|parent| self.reply_reason(parent))
    }

    fn reply_reason(&self, parent: &str) -> Option<&'static str> {
        let issue = self.snapshot.issue.as_ref()?;
        let root = issue
            .timeline
            .page()
            .into_iter()
            .flat_map(|page| &page.items)
            .filter_map(|entry| entry.comment.as_ref())
            .chain(issue.timeline.projected())
            .find(|comment| comment.id == parent);
        match root {
            Some(root) if root.parent_comment_id.is_some() || root.deleted_at.is_some() => {
                Some("Reply to a non-deleted root comment.")
            }
            Some(root) if root.resolved => Some("This thread is resolved."),
            Some(_) => None,
            None => Some("Load the discussion before replying to this thread."),
        }
    }
}

impl Hub {
    pub fn has_comment_drafts(&self) -> bool {
        lock(&self.0.state)
            .drafts
            .values()
            .any(|draft| !draft.body().is_empty() || draft.is_frozen() || draft.has_attachments())
    }
    pub fn comment_draft(&self, issue: &str, parent: Option<&str>) -> Option<CommentDraft> {
        lock(&self.0.state)
            .drafts
            .get(&DraftKey::new(issue, parent))
            .cloned()
    }
    pub fn comment_reason(&self, issue: &str, parent: Option<&str>) -> Option<&'static str> {
        lock(&self.0.state).writing_reason(issue, parent)
    }
    // qual:api — callers without a retained view edit the current generation.
    pub fn edit_comment(
        &self,
        issue: &str,
        parent: Option<&str>,
        text: &str,
    ) -> Result<(), HubError> {
        self.edit_comment_at(self.snapshot().generation(), issue, parent, text)
    }
    pub fn edit_comment_at(
        &self,
        generation: super::ScopeGeneration,
        issue: &str,
        parent: Option<&str>,
        text: &str,
    ) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        if !state.current(generation) {
            return Ok(());
        }
        let draft = state
            .drafts
            .entry(DraftKey::new(issue, parent))
            .or_insert_with(CommentDraft::new);
        draft.edit(text);
        state.changed();
        Ok(())
    }
    pub fn discard_comment(&self, issue: &str, parent: Option<&str>) {
        let mut state = lock(&self.0.state);
        let key = DraftKey::new(issue, parent);
        if let Some(draft) = state
            .drafts
            .get_mut(&key)
            .filter(|d| !d.is_frozen() && d.has_attachments())
        {
            draft.edit("");
            draft.lease = images::next_id();
            draft.intakes = 0;
            let images = draft
                .attachments()
                .iter()
                .map(|image| image.id())
                .collect::<Vec<_>>();
            if images.is_empty() {
                state.drafts.remove(&key);
            }
            state.changed();
            drop(state);
            for id in images {
                self.remove_comment_image(issue, parent, &id);
            }
            return;
        }
        if state
            .drafts
            .get(&key)
            .is_some_and(|draft| draft.state() != &CommentState::Posting && !draft.checking())
        {
            state.drafts.remove(&key);
            state.changed();
        }
    }
}
