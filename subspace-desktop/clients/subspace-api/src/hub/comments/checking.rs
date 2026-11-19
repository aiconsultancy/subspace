use super::{CommentDraft, CommentState, DraftKey};
use crate::{
    Error,
    hub::{
        Hub, HubError, ReadPriority, Shared, lock,
        worker::{Job, Task},
    },
    model::Comment,
};
use std::collections::HashSet;

impl Hub {
    /// This is a read. A completed scan enables a separate, explicit Post again.
    pub fn check_comment(&self, issue: &str, parent: Option<&str>) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let key = DraftKey::new(issue, parent);
        let Some(draft) = state.drafts.get_mut(&key) else {
            return Ok(());
        };
        if !matches!(draft.state, CommentState::Unclear { .. }) || draft.checking {
            return Ok(());
        }
        draft.checking = true;
        draft.state = CommentState::Unclear { refresh: false };
        state.changed();
        self.0
            .enqueue(&mut state, Task::Discussion(key), ReadPriority::Visible);
        Ok(())
    }
    // qual:api — named alias for the Check discussion action.
    pub fn check_discussion(&self, issue: &str, parent: Option<&str>) -> Result<(), HubError> {
        self.check_comment(issue, parent)
    }
}

pub(in crate::hub) fn check(shared: &Shared, job: &Job, key: &DraftKey) {
    let input = {
        let state = lock(&shared.state);
        state
            .snapshot
            .me
            .as_ref()
            .zip(state.drafts.get(key))
            .map(|(me, draft)| (me.user_id.clone(), draft.clone()))
    };
    let Some((user, draft)) = input else {
        return;
    };
    let result = find_post(&shared.client, key, &user, &draft);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if let Some(current) = state
        .drafts
        .get_mut(key)
        .filter(|current| current.started == draft.started && current.checking)
    {
        current.checking = false;
        match &result {
            Ok(Some(_)) => current.finish(&Ok(crate::CommentPost::AlreadyPosted)),
            Ok(None) => {
                current.state = CommentState::Unclear { refresh: true };
                current.notice =
                    Some("No matching comment was found. Post again may post the comment twice.");
            }
            Err(_) => {
                current.refresh_failed = true;
                current.notice = Some("Couldn't check discussion. Check again before posting.");
            }
        }
    }
    let confirmed_uploads = confirmed_media(&draft, &result);
    let checked = result.is_ok();
    if let Ok(Some(comment)) = result
        && let Some(issue) = state.snapshot.issue.as_mut()
        && issue.detail.as_ref().is_some_and(|detail| {
            detail.fields.id == key.issue || detail.fields.identifier == key.issue
        })
    {
        issue.timeline.content.projected.push(comment);
    }
    state.changed();
    if checked {
        refresh_checked(shared, &mut state, key, confirmed_uploads);
    }
}

fn find_post(
    client: &crate::Client,
    key: &DraftKey,
    user: &str,
    draft: &CommentDraft,
) -> Result<Option<Comment>, Error> {
    let existing_ids = draft.existing_ids.as_ref().ok_or(Error::InvalidRequest)?;
    super::reading::scan(client, key, |comment| {
        matches_post(
            comment,
            key,
            (user, draft.submitted.as_deref().unwrap_or(&draft.body)),
            existing_ids,
        )
    })
}

fn matches_post(
    comment: &Comment,
    key: &DraftKey,
    content: (&str, &str),
    existing_ids: &HashSet<String>,
) -> bool {
    comment.actual_author.user_id == content.0
        && comment.body == content.1
        && comment.parent_comment_id == key.parent
        && comment.deleted_at.is_none()
        && !existing_ids.contains(&comment.id)
}

fn confirmed_media(draft: &CommentDraft, result: &Result<Option<Comment>, Error>) -> bool {
    draft.has_attachments() && result.as_ref().is_ok_and(|comment| comment.is_some())
}
fn refresh_checked(shared: &Shared, state: &mut crate::hub::State, key: &DraftKey, media: bool) {
    if media {
        state.refresh_media(&key.issue);
    }
    shared.refresh_comments(state, &key.issue);
}
