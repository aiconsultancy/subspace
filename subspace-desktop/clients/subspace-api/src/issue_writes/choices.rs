//! Demand-only, paged property catalogues. Names never enter write payloads.
use super::IssueSaveError;
mod planning;
use crate::query::{MAX_LIST_PAGE_SIZE, MAX_SCAN_PAGES};
use crate::{
    Client, Error, PageRequest,
    model::{Label, Page, WorkflowState},
};
use serde::Deserialize;
const FIBONACCI: &[i32] = &[1, 2, 3, 5, 8];
const EXPONENTIAL: &[i32] = &[1, 2, 4, 8, 16];
const LINEAR: &[i32] = &[1, 2, 3, 4, 5];
use std::collections::HashSet;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ChoiceSource {
    States(String),
    Assignees,
    Labels,
    Estimates(String),
    Milestones(String),
    Cycles(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct IssueChoice {
    id: String,
    name: String,
}
impl IssueChoice {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}
impl std::fmt::Debug for IssueChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IssueChoice([redacted])")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    id: String,
    display_name: String,
    kind: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EstimateProject {
    estimate_scale: Option<String>,
}

// qual:allow(coupling, oi) reason: "The catalogue boundary pages service records and projects them to immutable picker values."
impl Client {
    pub fn issue_choices(&self, source: &ChoiceSource) -> Result<Vec<IssueChoice>, IssueSaveError> {
        if self.backend == crate::Backend::Nett {
            return self.nett_choices(source);
        }
        match source {
            ChoiceSource::States(project) => {
                let rows: Vec<WorkflowState> = all_pages(|p| self.workflow_states(project, p))?;
                Ok(rows
                    .into_iter()
                    .map(|s| IssueChoice::new(s.id, s.name))
                    .collect())
            }
            ChoiceSource::Assignees => {
                let rows: Vec<User> =
                    all_pages(|p| self.get(&["users"], &p.parameters(MAX_LIST_PAGE_SIZE)?))?;
                Ok(rows
                    .into_iter()
                    .filter(|p| matches!(p.kind.as_str(), "human" | "agent"))
                    .map(|p| {
                        IssueChoice::new(
                            p.id,
                            if p.kind == "agent" {
                                format!("{} (agent)", p.display_name)
                            } else {
                                p.display_name
                            },
                        )
                    })
                    .collect())
            }
            ChoiceSource::Labels => {
                let rows: Vec<Label> = all_pages(|p| {
                    let mut query = p.parameters(MAX_LIST_PAGE_SIZE)?;
                    query.push(("state".into(), "active".into()));
                    self.get(&["labels"], &query)
                })?;
                Ok(rows
                    .into_iter()
                    .filter(|l| l.state == "active")
                    .map(|l| IssueChoice::new(l.id, l.name))
                    .collect())
            }
            ChoiceSource::Milestones(project) => self.milestone_choices(project),
            ChoiceSource::Cycles(project) => self.cycle_choices(project),
            ChoiceSource::Estimates(project) => {
                let row: EstimateProject = self.get(&["projects", project], &[])?;
                Ok(estimate_choices(row.estimate_scale.as_deref()))
            }
        }
    }
}

// qual:allow(coupling, oi) reason: "NETT keeps people, labels and estimates in other collections; this maps them to the same picker values."
impl Client {
    fn nett_choices(&self, source: &ChoiceSource) -> Result<Vec<IssueChoice>, IssueSaveError> {
        let named = |rows: Vec<(String, String)>| {
            rows.into_iter()
                .map(|(id, name)| IssueChoice::new(id, name))
                .collect()
        };
        Ok(match source {
            ChoiceSource::Assignees => named(self.nett_members()?),
            ChoiceSource::Labels => self
                .nett_labels()?
                .into_iter()
                .map(|label| IssueChoice::new(label.id, label.name))
                .collect(),
            ChoiceSource::Estimates(team) => {
                estimate_choices(self.nett_estimate_scale(team)?.as_deref())
            }
            // NETT milestones belong to planning projects, not teams.
            ChoiceSource::Milestones(_) => Vec::new(),
            ChoiceSource::States(team) => {
                let rows: Vec<WorkflowState> = self.nett_workflow_states(team)?.items;
                rows.into_iter()
                    .map(|s| IssueChoice::new(s.id, s.name))
                    .collect()
            }
            ChoiceSource::Cycles(team) => self.cycle_choices(team)?,
        })
    }
}

pub(crate) fn all_pages<T>(
    mut read: impl FnMut(&PageRequest) -> Result<Page<T>, Error>,
) -> Result<Vec<T>, Error> {
    let mut request = PageRequest {
        limit: MAX_LIST_PAGE_SIZE,
        cursor: None,
    };
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    loop {
        let page = read(&request)?;
        rows.extend(page.items);
        let Some(cursor) = page.next_cursor else {
            return Ok(rows);
        };
        if !seen.insert(cursor.clone()) || seen.len() >= MAX_SCAN_PAGES {
            return Err(Error::InvalidRequest);
        }
        request.cursor = Some(cursor);
    }
}

fn estimate_choices(scale: Option<&str>) -> Vec<IssueChoice> {
    let values: &[i32] = match scale {
        Some("fibonacci" | "tShirt") => FIBONACCI,
        Some("exponential") => EXPONENTIAL,
        Some("linear") => LINEAR,
        _ => return Vec::new(),
    };
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let label = if scale == Some("tShirt") {
                ["XS", "S", "M", "L", "XL"][i].to_owned()
            } else {
                v.to_string()
            };
            IssueChoice::new(v.to_string(), label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::all_pages;
    use crate::{Error, model::Page, query::MAX_SCAN_PAGES};

    #[test]
    fn scans_reject_unique_cursors_after_a_thousand_pages() {
        let mut calls = 0;
        let result = all_pages::<()>(|_| {
            calls += 1;
            Ok(Page {
                items: vec![],
                next_cursor: Some(calls.to_string()),
            })
        });
        assert!(matches!(result, Err(Error::InvalidRequest)));
        assert_eq!(calls, MAX_SCAN_PAGES);
    }

    #[test]
    fn scans_accept_a_complete_thousandth_page_and_reject_cycles() {
        let mut calls = 0;
        let result = all_pages(|_| {
            calls += 1;
            Ok(Page {
                items: vec![calls],
                next_cursor: (calls < MAX_SCAN_PAGES).then(|| calls.to_string()),
            })
        });
        assert_eq!(result.unwrap().len(), MAX_SCAN_PAGES);
        let result = all_pages::<()>(|_| {
            Ok(Page {
                items: vec![],
                next_cursor: Some("cycle".into()),
            })
        });
        assert!(matches!(result, Err(Error::InvalidRequest)));
    }
}
