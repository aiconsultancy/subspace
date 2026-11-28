//! What the board asks Subspace API for. With a hub, each request becomes a hub
//! call at once; without one, as in the board's own tests, they wait.

use subspace_api_client::hub::ActivitySection;
use subspace_api_client::{ActivityQuery, ListQuery};

use crate::model::{ProjectScope, SubIssues, Tab};

// qual:allow(coupling, oi) reason: "The board owns conversion of its retained controls into an owned list request."
impl crate::Issues {
    /// The requests made since the last call, oldest first.
    pub(crate) fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    // qual:allow(dry, boilerplate) reason: "An owned request snapshots the mutable toolbar; the queued query must not borrow the board."
    pub(crate) fn choice(&self) -> ListChoice {
        ListChoice {
            tab: self.tab,
            project: self.project.clone(),
            search: self.query.clone(),
            sub_issues: self.sub_issues,
            filters: self.filters.clone(),
            initiative: crate::initiative::list_initiative(self),
        }
    }
}

/// The longest search Subspace API accepts, in UTF-16 units.
const MAX_SEARCH_UNITS: usize = 200;

/// The choices a list read follows: the tab's predicate, the project, the
/// search text and whether sub-issues show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListChoice {
    pub(crate) tab: Tab,
    pub(crate) project: ProjectScope,
    pub(crate) search: String,
    pub(crate) sub_issues: SubIssues,
    pub(crate) filters: crate::filters::Filters,
    /// A NETT initiative the list keeps to; Subspace API scopes by project.
    pub(crate) initiative: Option<String>,
}

impl ListChoice {
    /// The hub's query: no project means All projects.
    pub(crate) fn query(&self) -> ListQuery {
        let project = match &self.project {
            ProjectScope::Project(key) => Some(key.as_str()),
            ProjectScope::AllProjects => None,
        };
        self.filters.apply(
            ListQuery::new(self.tab.spec().list)
                .project(project)
                .initiative(self.initiative.as_deref())
                .search(self.search.clone())
                .show_subissues(self.sub_issues == SubIssues::Nested),
        )
    }
}

/// Search text as Subspace API takes it: trimmed, and cut at 200 UTF-16 units on a
/// character boundary, so a long paste never makes the read fail.
pub(crate) fn search_text(text: &str) -> String {
    let mut units = 0;
    text.trim()
        .chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= MAX_SEARCH_UNITS
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    /// Retry the API connection and project catalogue.
    Access,
    OpenIssue {
        identifier: String,
    },
    ReloadIssue,
    Activity {
        query: ActivityQuery,
        reload: bool,
    },
    ActivityPage {
        section: ActivitySection,
        more: bool,
    },
    MoreSubissues,
    MoreRelations,
    MorePullRequests,
    /// Check or uncheck a to-do on the open issue.
    SetTodo {
        item: String,
        done: bool,
    },
    RetryTodos,
    OpenTimeline(usize),
    MoreTimeline(usize),
    RetryTimeline(usize),
    OpenDecisions,
    MoreDecisions,
    RetryDecisions,
    OpenDecision(String),
    RetryDecision(String),
    CloseDecision,
    /// The visible list, when its choices changed or its last read is over
    /// a minute old.
    List(ListChoice),
    /// The visible list and the projects, read again: Reload and `r`.
    Reload(ListChoice),
    ReloadProjects,
    /// The next page of one group, near its end.
    More {
        group: String,
    },
    /// A folded Done, Cancelled or Duplicate group's first page, once
    /// unfolded.
    Group {
        group: String,
    },
}

#[cfg(test)]
#[path = "tests/requests.rs"]
mod tests;
