use std::path::Path;

use rusqlite::Connection;

use crate::{Document, Policy, database, files, limits, storage_error};

const MAX_DOCUMENT: usize = 128 * 1024;
const MAX_BATCH: usize = 1024 * 1024;
const JOURNAL_RESERVE: u64 = 65_536;

pub(crate) fn reserve(connection: &Connection, path: &Path, policy: &Policy) -> Result<(), String> {
    let pages = database::pages(connection)?;
    // Filesystem preallocation and sidecars count once. The rollback journal
    // records SQLite pages, so only the page budget needs the storage factor.
    let extra_allocation = files::usage(path)?.saturating_sub(pages.bytes());
    let max_pages = policy
        .max_bytes
        .saturating_sub(JOURNAL_RESERVE)
        .saturating_sub(extra_allocation)
        / limits::STORAGE_FACTOR
        / pages.size;
    if max_pages < pages.count {
        return Err("Search quota cannot admit this write; clear cache or increase quota".into());
    }
    connection
        .pragma_update(
            None,
            "max_page_count",
            i64::try_from(max_pages).map_err(storage_error)?,
        )
        .map_err(storage_error)
}

pub(crate) fn encode(documents: &[Document]) -> Result<Vec<String>, String> {
    if documents.len() > limits::MAX_RESULTS {
        return Err("Search write exceeds batch limit".into());
    }
    let mut total = 0_usize;
    let mut encoded = Vec::with_capacity(documents.len());
    for document in documents {
        if raw_size(document) > MAX_DOCUMENT {
            return Err("Search document exceeds supported bounds".into());
        }
        let json = serde_json::to_string(document).map_err(storage_error)?;
        total = total.saturating_add(json.len());
        if json.len() > MAX_DOCUMENT
            || total > MAX_BATCH
            || document.id.is_empty()
            || document.id.len() > limits::MAX_IDENTIFIER
            || document.refreshed_at > crate::now()
        {
            return Err("Search document exceeds supported bounds".into());
        }
        encoded.push(json);
    }
    Ok(encoded)
}

fn raw_size(document: &Document) -> usize {
    [
        &document.id,
        &document.title,
        &document.body,
        &document.metadata,
        &document.kind,
        &document.navigation,
    ]
    .into_iter()
    .chain(document.parent_id.iter())
    .chain(document.url.iter())
    .chain(document.payload.iter())
    .fold(0_usize, |size, text| size.saturating_add(text.len()))
}
