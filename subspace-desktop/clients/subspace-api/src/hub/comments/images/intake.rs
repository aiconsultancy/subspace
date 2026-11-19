use super::{CommentDraft, DraftKey, Hub, ScopeGeneration, lock};
use crate::uploads::{ImageDeclaration, UploadError};
use std::sync::Arc;

pub struct ImageIntake {
    hub: Hub,
    generation: ScopeGeneration,
    key: DraftKey,
    lease: u64,
}
impl Hub {
    pub fn begin_comment_image_intake(
        &self,
        generation: ScopeGeneration,
        issue: &str,
        parent: Option<&str>,
    ) -> Result<ImageIntake, UploadError> {
        let mut state = lock(&self.0.state);
        if !state.current(generation) || state.writing_reason(issue, parent).is_some() {
            return Err(UploadError::Removed);
        }
        let key = DraftKey::new(issue, parent);
        let draft = state
            .drafts
            .entry(key.clone())
            .or_insert_with(CommentDraft::new);
        if draft.is_frozen() {
            return Err(UploadError::Removed);
        }
        draft.intakes = draft.intakes.saturating_add(1);
        let lease = draft.lease;
        state.changed();
        Ok(ImageIntake {
            hub: self.clone(),
            generation,
            key,
            lease,
        })
    }
}
impl ImageIntake {
    pub fn current(&self) -> bool {
        let state = lock(&self.hub.0.state);
        state.current(self.generation)
            && state
                .drafts
                .get(&self.key)
                .is_some_and(|d| d.lease == self.lease && !d.is_frozen())
    }
    pub fn attach(
        &self,
        declaration: ImageDeclaration,
        bytes: Arc<[u8]>,
    ) -> Result<String, UploadError> {
        if !declaration.unchanged(&bytes) {
            return Err(UploadError::InvalidImage);
        }
        self.hub
            .attach_image_at(self.generation, (&self.key, self.lease), declaration, bytes)
    }
}
impl Drop for ImageIntake {
    fn drop(&mut self) {
        let mut state = lock(&self.hub.0.state);
        if state.current(self.generation)
            && let Some(draft) = state
                .drafts
                .get_mut(&self.key)
                .filter(|d| d.lease == self.lease)
        {
            draft.intakes = draft.intakes.saturating_sub(1);
            state.changed();
        }
    }
}
