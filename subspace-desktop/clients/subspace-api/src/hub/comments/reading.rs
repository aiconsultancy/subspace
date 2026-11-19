//! A complete thread scan; cursors and requests retain the same root scope.
use super::DraftKey;
use crate::{Client, Error, PageRequest, model::Comment};
use std::collections::HashSet;

pub(super) fn existing_ids(client: &Client, key: &DraftKey) -> Result<HashSet<String>, Error> {
    let mut ids = HashSet::new();
    scan(client, key, |comment| {
        ids.insert(comment.id.clone());
        false
    })?;
    Ok(ids)
}

pub(super) fn scan(
    client: &Client,
    key: &DraftKey,
    mut matches: impl FnMut(&Comment) -> bool,
) -> Result<Option<Comment>, Error> {
    let mut request = PageRequest {
        limit: crate::query::MAX_COMMENT_PAGE_SIZE,
        cursor: None,
    };
    let mut cursors = HashSet::new();
    loop {
        let page = match key.parent.as_deref() {
            Some(root) => client.replies(&key.issue, root, &request)?,
            None => client.comments(&key.issue, &request)?,
        };
        if let Some(comment) = page.items.into_iter().find(&mut matches) {
            return Ok(Some(comment));
        }
        let Some(cursor) = page.next_cursor else {
            return Ok(None);
        };
        if !cursors.insert(cursor.clone()) {
            return Err(Error::InvalidResponse);
        }
        request.cursor = Some(cursor);
    }
}
