//! The tracker's reads: the caller, initiatives and fleets, projects and
//! their workflow states, issues and their groups, decisions, comments,
//! relations and pull requests. NETT answers from its own routes.
use crate::model::{
    self, Comment, IssueDetail, IssueGroups, IssueListItem, Me, Page, Project, Relation,
    WorkflowState,
};
use crate::{
    Backend, Client, DecisionQuery, Error, IssueQuery, ListQuery, PageRequest, backend, nett, query,
};
use std::collections::HashSet;

// qual:allow(coupling, oi) reason: "The tracker's reads are one concern of the client, kept apart from its construction and transport in lib.rs."
impl Client {
    pub fn me(&self) -> Result<Me, Error> {
        if self.backend == Backend::Nett {
            return self.nett_me();
        }
        self.get(&["me"], &[])
    }

    /// Initiative memberships of the current Subspace API caller. An item
    /// that names only its fleet's ID gets the fleet read separately. If that
    /// read fails, the item keeps its `fleet_id` with no `fleet`: callers must
    /// treat it as unresolved, never as the default server, and the shared
    /// initiative catalogue reads it again with backoff.
    pub fn member_initiatives(&self) -> Result<Vec<model::MemberInitiative>, Error> {
        if self.backend == Backend::Nett
            && !self.nett_serves(backend::Feature::InitiativeChannels)?
        {
            return self.nett_initiatives();
        }
        let mut items: Vec<model::MemberInitiative> = if self.backend == Backend::Nett {
            // NETT pages its initiatives with channels; Subspace API returns them all.
            self.nett_all(&["me", "initiatives"], &[])?
                .into_iter()
                .map(|mut item| {
                    nett::wire::label_initiative(&mut item);
                    serde_json::from_value(item)
                })
                .collect::<Result<_, _>>()
                .map_err(|_| Error::InvalidResponse)?
        } else {
            let page: Page<model::MemberInitiative> = self.get(&["me", "initiatives"], &[])?;
            page.items
        };
        for item in items.iter_mut().filter(|item| item.fleet.is_none()) {
            if let Some(id) = item.fleet_id.clone() {
                item.fleet = self.fleet(&id).ok();
            }
        }
        Ok(items)
    }

    /// One fleet by ID or slug.
    pub fn fleet(&self, id: &str) -> Result<model::Fleet, Error> {
        if !self.serves(backend::Feature::InitiativeChannels) {
            return Err(Error::NotFound);
        }
        self.get(&["fleets", id], &[])
    }

    pub fn projects(&self, page: &PageRequest) -> Result<Page<Project>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_teams(page);
        }
        let mut parameters = page.parameters(query::MAX_LIST_PAGE_SIZE)?;
        parameters.push(("includeArchived".into(), "true".into()));
        self.get(&["projects"], &parameters)
    }

    /// Read all project pages and reject a cursor that repeats.
    pub fn all_projects(&self) -> Result<Vec<Project>, Error> {
        let mut request = PageRequest {
            limit: query::MAX_LIST_PAGE_SIZE,
            cursor: None,
        };
        let mut projects = vec![];
        let mut cursors = HashSet::new();
        loop {
            let page = self.projects(&request)?;
            projects.extend(page.items);
            let Some(cursor) = page.next_cursor else {
                return Ok(projects);
            };
            if !cursors.insert(cursor.clone()) {
                return Err(Error::InvalidResponse);
            }
            request.cursor = Some(cursor);
        }
    }

    pub fn workflow_states(
        &self,
        project: &str,
        page: &PageRequest,
    ) -> Result<Page<WorkflowState>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_workflow_states(project);
        }
        self.get(
            &["projects", project, "workflow-states"],
            &page.parameters(query::MAX_LIST_PAGE_SIZE)?,
        )
    }

    pub fn issue_groups(&self, query: &ListQuery) -> Result<IssueGroups, Error> {
        self.get(&["issues", "groups"], &query.grouped_parameters()?)
    }

    pub(crate) fn count_groups(
        &self,
        parameters: &[(String, String)],
    ) -> Result<IssueGroups, Error> {
        self.get(&["issues", "groups"], parameters)
    }

    pub fn issue_group_page(
        &self,
        query: &ListQuery,
        group: &model::IssueGroup,
        cursor: Option<&str>,
    ) -> Result<Page<IssueListItem>, Error> {
        self.get(&["issues"], &query.continuation_parameters(group, cursor)?)
    }

    pub fn issues(&self, query: &IssueQuery) -> Result<Page<IssueListItem>, Error> {
        self.get(&["issues"], &query.parameters()?)
    }

    pub fn issue(&self, reference: &str) -> Result<IssueDetail, Error> {
        self.get(&["issues", reference], &[])
    }

    pub fn decisions(
        &self,
        query: &DecisionQuery,
        page: &PageRequest,
    ) -> Result<Page<model::DecisionListItem>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_decisions(query, page);
        }
        self.get(&["decisions"], &query.parameters(page)?)
    }

    pub fn decision(&self, id: &str) -> Result<model::DecisionDetail, Error> {
        if self.backend == Backend::Nett {
            return self.nett_decision(id);
        }
        self.get(&["decisions", id], &[])
    }

    pub fn decision_revisions(
        &self,
        id: &str,
        page: &PageRequest,
    ) -> Result<Page<model::DecisionRevision>, Error> {
        self.get(
            &["decisions", id, "revisions"],
            &page.parameters(query::MAX_DECISION_PAGE_SIZE)?,
        )
    }

    pub fn decision_revision(
        &self,
        id: &str,
        revision: &str,
    ) -> Result<model::DecisionRevisionDetail, Error> {
        self.get(&["decisions", id, "revisions", revision], &[])
    }

    pub fn comments(&self, reference: &str, page: &PageRequest) -> Result<Page<Comment>, Error> {
        self.get(
            &["issues", reference, "comments"],
            &page.parameters(query::MAX_COMMENT_PAGE_SIZE)?,
        )
    }

    pub fn replies(
        &self,
        reference: &str,
        root: &str,
        page: &PageRequest,
    ) -> Result<Page<Comment>, Error> {
        self.get(
            &["issues", reference, "comments", root, "replies"],
            &page.parameters(query::MAX_COMMENT_PAGE_SIZE)?,
        )
    }

    pub fn relations(&self, reference: &str, page: &PageRequest) -> Result<Page<Relation>, Error> {
        self.get(
            &["issues", reference, "relations"],
            &page.parameters(query::MAX_RELATION_PAGE_SIZE)?,
        )
    }

    pub fn pull_requests(
        &self,
        reference: &str,
        page: &PageRequest,
    ) -> Result<Page<model::PullRequest>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_pull_requests(reference, page);
        }
        self.get(
            &["issues", reference, "pull-requests"],
            &page.parameters(query::MAX_RELATION_PAGE_SIZE)?,
        )
    }
}

impl crate::reference::IssueResolver for Client {
    type Issue = IssueDetail;
    type Error = Error;
    fn resolve(&self, reference: &str) -> Result<IssueDetail, Error> {
        self.issue(reference)
    }
}
