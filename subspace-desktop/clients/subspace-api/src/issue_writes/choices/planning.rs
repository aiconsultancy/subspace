//! Project-scoped planning choices are read only when their picker opens.
use super::{IssueChoice, all_pages};
use crate::{Client, PageRequest, issue_writes::IssueSaveError};
use serde::Deserialize;

const PAGE_SIZE: u32 = 50;

#[derive(Deserialize)]
struct Milestone {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Cycle {
    id: String,
    number: u32,
    state: String,
}

// qual:allow(coupling, oi) reason: "The planning catalogue boundary composes scoped paged reads and projects service records into immutable picker choices."
impl Client {
    pub(crate) fn issue_milestone_name(
        &self,
        project: &str,
        id: &str,
    ) -> Result<Option<String>, crate::Error> {
        if self.backend == crate::Backend::Nett {
            return Ok(None);
        }
        let page: crate::model::Page<Milestone> = self.get(
            &["projects", project, "milestones"],
            &[
                ("milestoneId".into(), id.into()),
                ("limit".into(), "1".into()),
                ("includeContent".into(), "false".into()),
            ],
        )?;
        Ok(page.items.into_iter().find(|m| m.id == id).map(|m| m.name))
    }

    pub(super) fn milestone_choices(
        &self,
        project: &str,
    ) -> Result<Vec<IssueChoice>, IssueSaveError> {
        let rows: Vec<Milestone> = all_pages(|page| {
            let mut query = parameters(page)?;
            query.push(("includeContent".into(), "false".into()));
            self.get(&["projects", project, "milestones"], &query)
        })?;
        Ok(rows
            .into_iter()
            .map(|m| IssueChoice::new(m.id, m.name))
            .collect())
    }

    pub(super) fn cycle_choices(&self, project: &str) -> Result<Vec<IssueChoice>, IssueSaveError> {
        let rows: Vec<Cycle> = all_pages(|page| {
            let mut query = parameters(page)?;
            query.push(("project".into(), project.into()));
            self.get(&["issue-cycles"], &query)
        })?;
        Ok(rows
            .into_iter()
            .filter(|cycle| matches!(cycle.state.as_str(), "current" | "upcoming"))
            .map(|cycle| {
                IssueChoice::new(
                    cycle.id,
                    format!("Cycle {} · {}", cycle.number, cycle.state),
                )
            })
            .collect())
    }
}

fn parameters(page: &PageRequest) -> Result<Vec<(String, String)>, crate::Error> {
    PageRequest {
        limit: PAGE_SIZE,
        cursor: page.cursor.clone(),
    }
    .parameters(PAGE_SIZE)
}
