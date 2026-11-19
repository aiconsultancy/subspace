use super::{CommentState, DraftKey};
use crate::{
    CommentPost,
    hub::{Hub, HubError, Shared, lock},
};
use std::sync::Arc;

impl Hub {
    pub fn submit_comment(
        &self,
        issue: &str,
        parent: Option<&str>,
        retry: bool,
    ) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        if state.writing_reason(issue, parent).is_some() {
            return Ok(());
        }
        let key = DraftKey::new(issue, parent);
        let Some(draft) = state.drafts.get_mut(&key) else {
            return Ok(());
        };
        let intent = draft.begin(parent, retry);
        state.changed();
        let Some(intent) = intent else {
            return Ok(());
        };
        let generation = state.snapshot.generation;
        let shared = self.0.clone();
        std::thread::spawn(move || send(shared, generation, key, intent));
        Ok(())
    }
}

fn send(
    shared: Arc<Shared>,
    generation: crate::hub::ScopeGeneration,
    key: DraftKey,
    intent: crate::CommentIntent,
) {
    if !lock(&shared.state).current(generation) {
        return;
    }
    let result = post(&shared, generation, &key, &intent);
    let mut state = lock(&shared.state);
    if !state.current(generation) {
        return;
    }
    let mut had_uploads = false;
    if let Some(draft) = state
        .drafts
        .get_mut(&key)
        .filter(|draft| draft.state == CommentState::Posting)
    {
        had_uploads = draft.has_attachments();
        draft.finish(&result);
    }
    let success = result.is_ok();
    if let Ok(CommentPost::Created(comment)) = result
        && let Some(issue) = state.snapshot.issue.as_mut()
        && issue.detail.as_ref().is_some_and(|detail| {
            detail.fields.id == key.issue || detail.fields.identifier == key.issue
        })
    {
        issue.timeline.content.projected.push(*comment);
    }
    state.changed();
    if success {
        if had_uploads {
            state.refresh_media(&key.issue);
        }
        shared.refresh_comments(&mut state, &key.issue);
    }
}

fn post(
    shared: &Shared,
    generation: crate::hub::ScopeGeneration,
    key: &DraftKey,
    intent: &crate::CommentIntent,
) -> Result<CommentPost, crate::WriteError> {
    // Read before sending, so recovery cannot mistake an older identical post
    // for this attempt. Server and desktop timestamps need not agree.
    let ids = super::reading::existing_ids(&shared.client, key)
        .map_err(|_| crate::WriteError::NotSent)?;
    {
        let mut state = lock(&shared.state);
        if !state.current(generation) {
            return Err(crate::WriteError::NotSent);
        }
        let draft = state
            .drafts
            .get_mut(key)
            .ok_or(crate::WriteError::NotSent)?;
        draft.existing_ids = Some(ids);
    }
    shared.client.post_comment(&key.issue, intent)
}
