use crate::{Error, ListTab};

const STATE_TYPES: &[&str] = &[
    "triage",
    "backlog",
    "unstarted",
    "started",
    "completed",
    "canceled",
    "duplicate",
];
const MAX_VALUES: usize = 50;
const MAX_PRIORITY: u8 = 4;
const MAX_ASSIGNEE_BYTES: usize = 256;

#[derive(Clone, PartialEq, Eq, Default)]
pub(super) struct Toolbar {
    pub assignee: Option<String>,
    pub priorities: Vec<u8>,
    pub states: Vec<String>,
    pub state_types: Vec<String>,
    pub kind: Option<String>,
    pub due: Option<String>,
    pub include_archived: bool,
}

pub(super) fn canonical(values: &[&str]) -> Vec<String> {
    let mut values: Vec<_> = values.iter().map(|v| v.trim().to_owned()).collect();
    values.sort();
    values.dedup();
    values
}

impl Toolbar {
    pub fn parameters(&self, tab: ListTab, project: bool) -> Result<Vec<(String, String)>, Error> {
        self.validate(tab, project)?;
        let types = self.types(tab)?;
        let assignee = if tab == ListTab::MyIssues {
            Some("me")
        } else {
            self.assignee.as_deref()
        };
        let mut parameters = vec![];
        parameters.extend(assignee.map(|v| ("assignee".into(), v.into())));
        parameters.extend(
            self.priorities
                .iter()
                .map(|v| ("priority".into(), v.to_string())),
        );
        parameters.extend(self.states.iter().map(|v| ("state".into(), v.clone())));
        parameters.extend(types.into_iter().map(|v| ("stateType".into(), v)));
        parameters.extend(self.kind.iter().map(|v| ("type".into(), v.clone())));
        parameters.extend(self.due.iter().map(|v| ("due".into(), v.clone())));
        if self.include_archived {
            parameters.push(("includeArchived".into(), "true".into()));
        }
        Ok(parameters)
    }

    fn validate(&self, tab: ListTab, project: bool) -> Result<(), Error> {
        let invalid = self.assignee.as_deref().is_some_and(|v| {
            v.is_empty()
                || v.len() > MAX_ASSIGNEE_BYTES
                || !v
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        }) || (tab == ListTab::MyIssues && self.assignee.as_deref() == Some("none"))
            || self.invalid_values()
            || self.invalid_states(project);
        if invalid {
            Err(Error::InvalidRequest)
        } else {
            Ok(())
        }
    }

    fn invalid_states(&self, project: bool) -> bool {
        self.states.len() > MAX_VALUES
            || self.state_types.len() > MAX_VALUES
            || (!project && !self.states.is_empty())
            || self.states.iter().any(String::is_empty)
            || self
                .state_types
                .iter()
                .any(|v| !STATE_TYPES.contains(&v.as_str()))
    }

    fn invalid_values(&self) -> bool {
        self.priorities.iter().any(|v| *v > MAX_PRIORITY)
            || self
                .kind
                .as_deref()
                .is_some_and(|v| !matches!(v, "issue" | "tracker"))
            || self
                .due
                .as_deref()
                .is_some_and(|v| !matches!(v, "overdue" | "thisWeek" | "none"))
    }

    fn types(&self, tab: ListTab) -> Result<Vec<String>, Error> {
        let own: &[&str] = match tab {
            ListTab::Active => &["unstarted", "started"],
            ListTab::Backlog => &["backlog"],
            _ => &[],
        };
        if self.state_types.is_empty() {
            return Ok(own.iter().map(|v| (*v).into()).collect());
        }
        let types: Vec<_> = self
            .state_types
            .iter()
            .filter(|v| own.is_empty() || own.contains(&v.as_str()))
            .cloned()
            .collect();
        if types.is_empty() {
            Err(Error::InvalidRequest)
        } else {
            Ok(types)
        }
    }
}

impl std::fmt::Debug for Toolbar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toolbar")
            .field("assignee", &self.assignee)
            .field("priorities", &self.priorities)
            .field("state_count", &self.states.len())
            .field("state_type_count", &self.state_types.len())
            .finish_non_exhaustive()
    }
}
