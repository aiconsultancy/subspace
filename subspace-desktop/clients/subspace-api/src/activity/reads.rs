use super::{ActivityQuery, TIMELINE_PAGE_SIZE};
use crate::{
    Client, Error,
    model::{IssueListItem, Page},
};

// qual:allow(coupling, oi) reason: "This HTTP boundary composes the query encoder with shared authenticated transport, like the existing list endpoints."
impl Client {
    pub fn activity_issues(
        &self,
        query: &ActivityQuery,
        cursor: Option<&str>,
    ) -> Result<Page<IssueListItem>, Error> {
        self.get(&["issues"], &query.parameters(false, cursor))
    }
    pub fn trackers_in_progress(
        &self,
        query: &ActivityQuery,
        cursor: Option<&str>,
    ) -> Result<Page<IssueListItem>, Error> {
        self.get(&["issues"], &query.parameters(true, cursor))
    }

    pub fn issue_activity(
        &self,
        reference: &str,
        page: &crate::PageRequest,
    ) -> Result<crate::model::ActivityPage, Error> {
        self.get(
            &["issues", reference, "activity"],
            &page.parameters(TIMELINE_PAGE_SIZE)?,
        )
    }
}
