//! One list scope builds grouped first pages and ordinary group continuations.
use crate::{Error, model::IssueGroup};
mod filters;
use filters::Toolbar;

const FIRST_PAGE: u32 = 25;
pub(crate) const LATER_PAGE: u32 = 50;
const MAX_SEARCH: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListTab {
    MyIssues,
    #[default]
    Active,
    Backlog,
    All,
}

/// Memory-only filters. `None` project means All projects. The group dimension is
/// selected automatically; continuations replace only that dimension.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct ListQuery {
    tab: ListTab,
    project: Option<String>,
    /// A NETT initiative: issues in its planning projects. Subspace API
    /// scopes initiatives through their projects instead and never sets it.
    initiative: Option<String>,
    search: String,
    show_subissues: bool,
    toolbar: Toolbar,
}

// qual:allow(dry, boilerplate) reason: "Three native builders keep the list scope private without a derive dependency."
impl ListQuery {
    /// Me or no assignee. Invalid values are refused when the query is used.
    // qual:api — consumed by the slice 5 board toolbar.
    pub fn assignee(mut self, value: Option<&str>) -> Self {
        self.toolbar.assignee = value.map(str::to_owned);
        self
    }
    /// Subspace API priorities: 0 none, 1 urgent, 2 high, 3 medium, 4 low.
    // qual:api — consumed by the slice 5 board toolbar.
    pub fn priorities(mut self, values: &[u8]) -> Self {
        self.toolbar.priorities = values.to_vec();
        self.toolbar.priorities.sort_unstable();
        self.toolbar.priorities.dedup();
        self
    }
    /// State IDs or names in the selected project; use state_types for All projects.
    pub fn states(mut self, values: &[&str]) -> Self {
        self.toolbar.states = filters::canonical(values);
        self
    }
    /// Fixed Subspace API state types, intersected with the tab's own state types.
    pub fn state_types(mut self, values: &[&str]) -> Self {
        self.toolbar.state_types = filters::canonical(values);
        self
    }
    /// `issue` or `tracker`, or None for both.
    pub fn kind(mut self, value: Option<&str>) -> Self {
        self.toolbar.kind = value.map(str::to_owned);
        self
    }
    /// `overdue`, `thisWeek` or `none`, resolved using UTC dates.
    // qual:api — consumed by the slice 5 board toolbar.
    pub fn due(mut self, value: Option<&str>) -> Self {
        self.toolbar.due = value.map(str::to_owned);
        self
    }
    pub fn include_archived(mut self, include: bool) -> Self {
        self.toolbar.include_archived = include;
        self
    }
    pub fn new(tab: ListTab) -> Self {
        Self {
            tab,
            ..Self::default()
        }
    }
    pub fn project(mut self, project: Option<&str>) -> Self {
        self.project = project
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        self
    }
    /// Only issues in this NETT initiative's planning projects.
    pub fn initiative(mut self, initiative: Option<&str>) -> Self {
        self.initiative = initiative
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        self
    }
    pub fn search(mut self, search: impl Into<String>) -> Self {
        self.search = search.into().trim().into();
        self
    }
    pub fn show_subissues(mut self, show: bool) -> Self {
        self.show_subissues = show;
        self
    }
    pub fn selected_project(&self) -> Option<&str> {
        self.project.as_deref()
    }
    pub fn tab(&self) -> ListTab {
        self.tab
    }

    pub(crate) fn wider_search(&self) -> Option<Self> {
        (self.tab != ListTab::All && !self.search.is_empty()).then(|| Self {
            tab: ListTab::All,
            ..self.clone()
        })
    }

    pub fn grouped_parameters(&self) -> Result<Vec<(String, String)>, Error> {
        let mut parameters = self.filters()?;
        parameters.push(("groupBy".into(), self.dimension().into()));
        parameters.push(("limitPerGroup".into(), FIRST_PAGE.to_string()));
        Ok(parameters)
    }

    pub(crate) fn hidden_count_parameters(&self) -> Result<Option<Vec<(String, String)>>, Error> {
        if self.show_subissues {
            return Ok(None);
        }
        let mut parameters = self.grouped_parameters()?;
        parameters.retain(|(key, _)| key != "parent" && key != "limitPerGroup");
        parameters.push(("limitPerGroup".into(), "0".into()));
        Ok(Some(parameters))
    }

    pub(crate) fn my_count_parameters() -> Vec<(String, String)> {
        [
            ("groupBy", "stateType"),
            ("assignee", "me"),
            ("stateType", "triage"),
            ("stateType", "backlog"),
            ("stateType", "unstarted"),
            ("stateType", "started"),
            ("limitPerGroup", "0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }

    pub fn continuation_parameters(
        &self,
        group: &IssueGroup,
        cursor: Option<&str>,
    ) -> Result<Vec<(String, String)>, Error> {
        let mut parameters = self.filters()?;
        let dimension = self.dimension();
        let selector = if dimension == "state" {
            "stateId"
        } else {
            dimension
        };
        parameters.retain(|(key, _)| key != selector && key != dimension);
        let value = if self.project.is_some() {
            group.state_id.as_deref().ok_or(Error::InvalidRequest)?
        } else {
            &group.state_type
        };
        parameters.push((selector.into(), value.into()));
        parameters.push(("limit".into(), LATER_PAGE.to_string()));
        parameters.extend(cursor.map(|v| ("cursor".into(), v.into())));
        Ok(parameters)
    }

    fn dimension(&self) -> &'static str {
        if self.project.is_some() {
            "state"
        } else {
            "stateType"
        }
    }

    fn filters(&self) -> Result<Vec<(String, String)>, Error> {
        if self.search_too_long() {
            return Err(Error::InvalidRequest);
        }
        let mut parameters = vec![("order".into(), self.order().into())];
        parameters.extend(
            self.project
                .iter()
                .map(|project| ("project".into(), project.clone())),
        );
        parameters.extend(
            self.initiative
                .iter()
                .map(|initiative| ("initiative".into(), initiative.clone())),
        );
        if !self.show_subissues {
            parameters.push(("parent".into(), "none".into()));
        }
        if !self.search.is_empty() {
            parameters.push(("q".into(), self.search.clone()));
        }
        parameters.extend(self.toolbar.parameters(self.tab, self.project.is_some())?);
        Ok(parameters)
    }

    pub(crate) fn search_too_long(&self) -> bool {
        self.search.encode_utf16().count() > MAX_SEARCH
    }

    fn order(&self) -> &'static str {
        if self.tab == ListTab::Backlog && self.project.is_some() {
            "manual"
        } else {
            "priority"
        }
    }
}

impl std::fmt::Debug for ListQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListQuery")
            .field("tab", &self.tab)
            .field("project", &self.project)
            .field("initiative", &self.initiative)
            .field("search", &"[redacted]")
            .field("show_subissues", &self.show_subissues)
            .field("toolbar", &self.toolbar)
            .finish_non_exhaustive()
    }
}
