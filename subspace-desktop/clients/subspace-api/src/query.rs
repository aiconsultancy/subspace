//! Finite paging and the filters needed by one project's issue list.

use crate::Error;

pub(crate) const MAX_SCAN_PAGES: usize = 1000;
pub(crate) const MAX_LIST_PAGE_SIZE: u32 = 200;
pub(crate) const MAX_COMMENT_PAGE_SIZE: u32 = 200;
pub(crate) const MAX_RELATION_PAGE_SIZE: u32 = 100;
const MAX_SEARCH_LENGTH: usize = 200;
pub const MAX_DECISION_PAGE_SIZE: u32 = 100;

/// Decision filters use the API's repeated status query parameters.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct DecisionQuery {
    pub issue: Option<String>,
    pub project: Option<String>,
    pub status: Vec<crate::model::DecisionStatus>,
    pub made_by_type: Option<crate::model::DecisionMakerType>,
    pub tag: Option<String>,
    pub q: Option<String>,
}

impl DecisionQuery {
    pub(crate) fn parameters(&self, page: &PageRequest) -> Result<Vec<(String, String)>, Error> {
        let mut query = Vec::new();
        for (key, value) in [
            ("issue", &self.issue),
            ("project", &self.project),
            ("tag", &self.tag),
            ("q", &self.q),
        ] {
            query.extend(value.iter().map(|value| (key.into(), value.clone())));
        }
        for status in &self.status {
            query.push(("status".into(), decision_status(*status)?.into()));
        }
        if let Some(kind) = self.made_by_type {
            use crate::model::DecisionMakerType;
            let value = match kind {
                DecisionMakerType::Human => "human",
                DecisionMakerType::Agent => "agent",
                DecisionMakerType::Other => return Err(Error::InvalidRequest),
            };
            query.push(("madeByType".into(), value.into()));
        }
        let bounded = PageRequest {
            limit: page.limit.min(MAX_DECISION_PAGE_SIZE),
            cursor: page.cursor.clone(),
        };
        query.extend(bounded.parameters(MAX_DECISION_PAGE_SIZE)?);
        Ok(query)
    }
}

fn decision_status(status: crate::model::DecisionStatus) -> Result<&'static str, Error> {
    use crate::model::DecisionStatus;
    [
        (DecisionStatus::Proposed, "proposed"),
        (DecisionStatus::Accepted, "accepted"),
        (DecisionStatus::Rejected, "rejected"),
        (DecisionStatus::Superseded, "superseded"),
    ]
    .into_iter()
    .find_map(|(candidate, value)| (candidate == status).then_some(value))
    .ok_or(Error::InvalidRequest)
}

#[derive(Clone)]
#[non_exhaustive]
pub struct PageRequest {
    pub limit: u32,
    pub cursor: Option<String>,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            limit: 50,
            cursor: None,
        }
    }
}

impl PageRequest {
    pub(crate) fn parameters(&self, maximum: u32) -> Result<Vec<(String, String)>, Error> {
        if !(1..=maximum).contains(&self.limit) {
            return Err(Error::InvalidRequest);
        }
        let mut query = vec![("limit".into(), self.limit.to_string())];
        query.extend(
            self.cursor
                .iter()
                .map(|cursor| ("cursor".into(), cursor.clone())),
        );
        Ok(query)
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct IssueQuery {
    pub project: String,
    pub parent: Option<String>,
    pub order: Option<String>,
    pub page: PageRequest,
    pub search: Option<String>,
    pub include_archived: bool,
}

impl IssueQuery {
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            parent: None,
            order: None,
            page: PageRequest::default(),
            search: None,
            include_archived: false,
        }
    }
    pub(crate) fn parameters(&self) -> Result<Vec<(String, String)>, Error> {
        let mut query = self.page.parameters(MAX_LIST_PAGE_SIZE)?;
        query.extend(self.filters()?);
        Ok(query)
    }

    fn filters(&self) -> Result<Vec<(String, String)>, Error> {
        let project = self.project.trim();
        if self
            .parent
            .as_ref()
            .is_some_and(|parent| parent.trim().is_empty())
            || self.order.as_deref().is_some_and(|order| {
                !matches!(order, "created" | "priority" | "manual" | "activity")
            })
        {
            return Err(Error::InvalidRequest);
        }
        let mut query = vec![("includeArchived".into(), self.include_archived.to_string())];
        if !project.is_empty() {
            query.push(("project".into(), project.into()));
        }
        query.extend(
            self.parent
                .iter()
                .map(|parent| ("parent".into(), parent.trim().into())),
        );
        query.extend(
            self.order
                .iter()
                .map(|order| ("order".into(), order.clone())),
        );
        query.extend(self.search()?.map(|search| ("q".into(), search.into())));
        Ok(query)
    }

    fn search(&self) -> Result<Option<&str>, Error> {
        let text = self
            .search
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        // Subspace API's .NET string.Length counts UTF-16 code units.
        if text.is_some_and(|text| text.encode_utf16().count() > MAX_SEARCH_LENGTH) {
            return Err(Error::InvalidRequest);
        }
        Ok(text)
    }
}

impl std::fmt::Debug for IssueQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueQuery")
            .field("project", &self.project)
            .field("parent", &self.parent)
            .field("page", &self.page)
            .field("search", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PageRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageRequest")
            .field("limit", &self.limit)
            .field("has_cursor", &self.cursor.is_some())
            .finish_non_exhaustive()
    }
}
