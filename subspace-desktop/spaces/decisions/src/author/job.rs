//! Background writes bound to the account that opened the dialog.

use std::collections::BTreeSet;

use subspace_api_client::decision_writes::{
    DecisionAttribution, DecisionChanges, DecisionReview, DecisionWriteError,
};
use subspace_api_client::model::DecisionDetail;
use subspace_api_client::{Client, Error};

use crate::load::Lease;

pub(crate) enum Job {
    Create {
        project: String,
        fields: Box<Fields>,
        attribution: Option<DecisionAttribution>,
    },
    Edit {
        target: Target,
        project: String,
        fields: Box<Fields>,
    },
    Review {
        target: Target,
        review: DecisionReview,
    },
}

#[derive(Clone)]
pub(crate) struct Target {
    pub(super) id: String,
    pub(super) version: u32,
}

pub(crate) struct Fields {
    pub(super) changes: DecisionChanges,
    pub(super) issues: Option<Vec<String>>,
}

#[derive(Debug)]
pub(crate) enum Problem {
    Write(DecisionWriteError),
    Link,
    Empty,
    ChooseProject,
}

impl Problem {
    pub(super) fn message(&self) -> String {
        match self {
            Self::Write(error) => error.to_string(),
            Self::Link => "Choose up to 100 available issues from the decision’s project.".into(),
            Self::Empty => "Change a field before saving.".into(),
            Self::ChooseProject => "Choose the project for this decision.".into(),
        }
    }
}

impl Job {
    pub(super) fn run(self, lease: &Lease) -> Result<DecisionDetail, Problem> {
        if lease.moved() {
            return Err(Problem::Write(DecisionWriteError::SignIn));
        }
        let client = Client::new()
            .map_err(|error| match error {
                Error::SignInNeeded | Error::OrganisationNeeded => DecisionWriteError::SignIn,
                _ => DecisionWriteError::NotSent,
            })
            .map_err(Problem::Write)?;
        if lease.moved() {
            return Err(Problem::Write(DecisionWriteError::SignIn));
        }
        match self {
            Self::Create {
                project,
                fields,
                attribution,
            } => {
                let changes = fields.resolve(&client, &project)?;
                current(lease)?;
                client
                    .propose_decision(&project, &changes, attribution.as_ref())
                    .map_err(Problem::Write)
            }
            Self::Edit {
                target,
                project,
                fields,
            } => {
                let changes = fields.resolve(&client, &project)?;
                current(lease)?;
                client
                    .edit_decision(&target.id, target.version, &changes)
                    .map_err(Problem::Write)
            }
            Self::Review { target, review } => client
                .review_decision(&target.id, target.version, review)
                .map_err(Problem::Write),
        }
    }
}

fn current(lease: &Lease) -> Result<(), Problem> {
    if lease.moved() {
        Err(Problem::Write(DecisionWriteError::SignIn))
    } else {
        Ok(())
    }
}

impl Fields {
    pub(super) fn changed(&self) -> bool {
        let changes = &self.changes;
        [
            &changes.title,
            &changes.decided_on,
            &changes.decision,
            &changes.context,
            &changes.why,
            &changes.implications,
            &changes.alternatives,
        ]
        .into_iter()
        .any(Option::is_some)
            || changes.tags.is_some()
            || changes.supersedes_id.is_some()
            || self.issues.is_some()
    }

    fn resolve(self, client: &Client, project: &str) -> Result<DecisionChanges, Problem> {
        let mut changes = self.changes;
        changes.issue_ids = self
            .issues
            .map(|refs| resolve_issues(client, project, refs))
            .transpose()?;
        Ok(changes)
    }
}

const MAX_LINKED_ISSUES: usize = 100;

fn resolve_issues(
    client: &Client,
    project: &str,
    refs: Vec<String>,
) -> Result<Vec<String>, Problem> {
    let refs: BTreeSet<_> = refs.into_iter().collect();
    if refs.len() > MAX_LINKED_ISSUES {
        return Err(Problem::Link);
    }
    refs.into_iter()
        .map(|key| {
            let issue = client.issue(&key).map_err(|error| match error {
                Error::Network | Error::UnexpectedStatus(_) => {
                    Problem::Write(DecisionWriteError::NotSent)
                }
                Error::SignInNeeded | Error::OrganisationNeeded => {
                    Problem::Write(DecisionWriteError::SignIn)
                }
                _ => Problem::Link,
            })?;
            (issue.fields.project_id == project)
                .then(|| issue.fields.id.clone())
                .ok_or(Problem::Link)
        })
        .collect()
}
