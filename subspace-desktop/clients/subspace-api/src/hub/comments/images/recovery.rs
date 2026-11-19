use super::{Attachment, AttachmentState, DraftKey};
use crate::query::MAX_SCAN_PAGES;
use crate::{Client, Error, PageRequest, hub::lock, model::IssueMedia, uploads::UploadError};
use std::collections::HashSet;
pub(super) fn check(
    client: &Client,
    key: &DraftKey,
    image: &Attachment,
) -> Result<(), UploadError> {
    {
        let mut row = lock(&image.inner);
        row.state = AttachmentState::Checking;
        row.checked_absent = false;
    }
    let rows = scan(client, &key.issue).map_err(|_| UploadError::Checking)?;
    let mut row = lock(&image.inner);
    let existing = row.existing.as_ref().ok_or(UploadError::Checking)?;
    let mut matches = rows
        .into_iter()
        .filter(|media| !existing.contains(&media.media_id) && row.declaration.matches(media));
    let Some(media) = matches.next() else {
        row.checked_absent = true;
        row.state = AttachmentState::NotFound;
        return Ok(());
    };
    if matches.next().is_some() {
        return Err(UploadError::Checking);
    }
    row.media = Some(media);
    row.state = AttachmentState::Ready;
    row.uncertain = false;
    Ok(())
}

pub(super) fn cleanup(
    client: &Client,
    key: &DraftKey,
    image: &Attachment,
) -> Result<(), UploadError> {
    let needs_check = {
        let row = lock(&image.inner);
        row.uncertain && !row.checked_absent
    };
    if needs_check {
        check(client, key, image)?;
    }
    let media = lock(&image.inner).media.clone();
    if let Some(media) = media {
        client
            .delete_media(&media)
            .map_err(|_| UploadError::Cleanup)?;
    }
    Ok(())
}

pub(super) fn scan(client: &Client, issue: &str) -> Result<Vec<IssueMedia>, Error> {
    let mut page = PageRequest::default();
    let mut cursors = HashSet::new();
    let mut rows = Vec::new();
    loop {
        let result = client.attachments(issue, &page)?;
        rows.extend(result.items);
        let Some(cursor) = result.next_cursor else {
            return Ok(rows);
        };
        if !cursors.insert(cursor.clone()) || cursors.len() >= MAX_SCAN_PAGES {
            return Err(Error::InvalidResponse);
        }
        page.cursor = Some(cursor);
    }
}
