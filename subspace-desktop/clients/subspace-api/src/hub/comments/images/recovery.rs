use super::{Attachment, AttachmentState, DraftKey};
use crate::query::MAX_SCAN_PAGES;
use crate::{Client, Error, PageRequest, hub::lock, model::IssueMedia, uploads::UploadError};
use std::collections::HashSet;
pub(super) fn check(
    client: &Client,
    key: &DraftKey,
    image: &Attachment,
) -> Result<(), UploadError> {
    let nett = {
        let mut row = lock(&image.inner);
        row.state = AttachmentState::Checking;
        row.checked_absent = false;
        row.request_key.clone().map(|request_key| {
            (
                row.media_draft.clone(),
                request_key,
                row.declaration.clone(),
            )
        })
    };
    if client.service() == crate::Backend::Nett {
        let found = match &nett {
            Some((draft, request_key, declaration)) => {
                client.nett_check_comment_media(&key.issue, draft, request_key, declaration)?
            }
            None => None,
        };
        let mut row = lock(&image.inner);
        match found {
            Some(media) => {
                row.media = Some(media);
                row.state = AttachmentState::Ready;
                row.uncertain = false;
            }
            None => {
                row.checked_absent = true;
                row.state = AttachmentState::NotFound;
            }
        }
        return Ok(());
    }
    let rows = scan(client, &key.issue).map_err(|_| UploadError::Checking)?;
    let mut row = lock(&image.inner);
    let existing = row.existing.as_ref().ok_or(UploadError::Checking)?;
    let mut matches = rows
        .into_iter()
        .filter(|media| row.can_claim(existing, media));
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
    if client.service() == crate::Backend::Nett {
        // A reservation is cancelled by its request key, whatever state it
        // reached; nothing was reserved without one.
        let reserved = {
            let row = lock(&image.inner);
            row.request_key.clone().map(|request_key| {
                (
                    row.media_draft.clone(),
                    request_key,
                    row.declaration.clone(),
                )
            })
        };
        return match reserved {
            Some((draft, request_key, declaration)) => {
                client.nett_cancel_comment_media(&key.issue, &draft, &request_key, &declaration)
            }
            None => Ok(()),
        };
    }
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
