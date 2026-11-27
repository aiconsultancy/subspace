//! Cursor pages of parent candidates, filtered by the Subspace API.
use crate::{
    Capabilities, Client, Error, IssueQuery,
    model::{IssueListItem, Page},
};

const PAGE_LIMIT: u32 = 20;

pub(crate) fn tracker_parents(
    client: &Client,
    search: &str,
    cursor: Option<String>,
    issue: Option<&str>,
) -> Result<Page<IssueListItem>, Error> {
    if !Capabilities::of(client.backend).issue_parent_search {
        return Err(Error::InvalidRequest);
    }
    let mut query = IssueQuery::new("").tracker_parents(issue.unwrap_or("new"));
    query.search = Some(search.to_owned());
    query.page.limit = PAGE_LIMIT;
    query.page.cursor = cursor;
    let mut page = client.issues(&query)?;
    if let Some(exclude) = issue {
        // Older servers may ignore parentFor; never offer the issue itself.
        page.items
            .retain(|item| item.fields.id != exclude && item.fields.identifier != exclude);
    }
    Ok(page)
}

#[cfg(test)]
#[path = "parent_search/tests.rs"]
mod tests;
