//! What the inspector shows of a decision: a peek drawn from the list read,
//! and the facts of an opened decision's detail. Pure, so the tests read
//! them without a window.

use subspace_api_client::model::{DecisionDetail, DecisionListItem, DecisionStatus, Project};
use subspace_framework::dashboard::dates;

use crate::words::{self, People};

/// A decision another one points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Related {
    pub(crate) id: String,
    pub(crate) title: String,
}

/// An issue the decision is linked to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Issue {
    pub(crate) identifier: String,
    pub(crate) title: String,
}

/// Where the decision's evidence came from, as far as it was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Evidence {
    pub(crate) kind: Option<&'static str>,
    pub(crate) reference: Option<String>,
    pub(crate) excerpt: Option<String>,
}

/// The selected decision at a glance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Peek {
    pub(crate) status: DecisionStatus,
    pub(crate) title: String,
    /// Decided on, made by, recorded by and project always; tags when there
    /// are any.
    pub(crate) facts: Vec<(&'static str, String)>,
    pub(crate) relations: Relations,
}

/// What the decision supersedes, what supersedes it, its linked issues and
/// its evidence: the parts the list read and the detail share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Relations {
    pub(crate) supersedes: Option<Related>,
    pub(crate) superseded_by: Option<Related>,
    pub(crate) issues: Vec<Issue>,
    /// Linked issues the read did not include.
    pub(crate) more_issues: usize,
    pub(crate) evidence: Option<Evidence>,
}

/// The peek of `item`.
pub(crate) fn peek(item: &DecisionListItem, people: &People, projects: &[Project]) -> Peek {
    let mut facts = vec![
        ("Decided on", words::decided_long(item)),
        ("Made by", words::maker_full(item, people)),
        (
            "Recorded by",
            words::recorder_id(&item.recorded_by_user_id, people),
        ),
        ("Project", words::project_of(item, projects)),
    ];
    facts.extend(tags(item));
    Peek {
        status: item.status,
        title: item.title.clone(),
        facts,
        relations: relations(item),
    }
}

/// The facts of an opened decision, which the inspector shows beside its
/// page.
pub(crate) fn facts(
    detail: &DecisionDetail,
    people: &People,
    projects: &[Project],
) -> Vec<(&'static str, String)> {
    let mut facts = vec![
        ("Status", words::status(detail.status).to_owned()),
        ("Decided on", words::decided_long(detail)),
        ("Made by", words::maker_full(detail, people)),
        ("Recorded by", words::recorder(&detail.recorded_by)),
    ];
    facts.extend(stamp(
        "Accepted",
        detail.accepted_at.as_deref(),
        detail.accepted_by_user_id.as_deref(),
        people,
    ));
    facts.extend(stamp(
        "Rejected",
        detail.rejected_at.as_deref(),
        detail.rejected_by_user_id.as_deref(),
        people,
    ));
    facts.push(("Project", words::project_of(detail, projects)));
    facts.extend(tags(detail));
    facts.push(("Revision", detail.revision_number.to_string()));
    facts
}

fn tags(item: &DecisionListItem) -> Option<(&'static str, String)> {
    (!item.tags.is_empty()).then(|| ("Tags", item.tags.join(", ")))
}

/// An accepted or rejected stamp: when, and by whom when that was recorded.
fn stamp(
    label: &'static str,
    at: Option<&str>,
    by: Option<&str>,
    people: &People,
) -> Option<(&'static str, String)> {
    let when = dates::stamp(at?);
    Some(match by {
        Some(id) => (
            label,
            format!("{when} by {}", words::person(Some(id), people)),
        ),
        None => (label, when),
    })
}

/// The relations of `item`; a link the read named without its summary
/// still shows.
pub(crate) fn relations(item: &DecisionListItem) -> Relations {
    let linked = &item.issues;
    let read = linked.items.len();
    Relations {
        supersedes: related(
            item.supersedes_id.as_deref(),
            item.supersedes
                .as_ref()
                .map(|summary| summary.title.as_str()),
            "An earlier decision",
        ),
        superseded_by: related(
            item.superseded_by_id.as_deref(),
            item.superseded_by
                .as_ref()
                .map(|summary| summary.title.as_str()),
            "A later decision",
        ),
        issues: linked
            .items
            .iter()
            .map(|issue| Issue {
                identifier: issue.identifier.clone(),
                title: issue.title.clone(),
            })
            .collect(),
        more_issues: usize::try_from(linked.count)
            .unwrap_or_default()
            .saturating_sub(read),
        evidence: evidence(item),
    }
}

fn related(id: Option<&str>, title: Option<&str>, fallback: &str) -> Option<Related> {
    Some(Related {
        id: id?.to_owned(),
        title: title.unwrap_or(fallback).to_owned(),
    })
}

fn evidence(item: &DecisionListItem) -> Option<Evidence> {
    let found = Evidence {
        kind: item.evidence_kind.map(words::evidence),
        reference: text(item.evidence_reference.as_deref()),
        excerpt: text(item.evidence_excerpt.as_deref()),
    };
    (found.kind.is_some() || found.reference.is_some() || found.excerpt.is_some()).then_some(found)
}

fn text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The sections of an opened decision's page that have something in them,
/// in the page's order.
pub(crate) fn prose(detail: &DecisionDetail) -> Vec<(&'static str, &str)> {
    [
        ("Decision", detail.decision.as_str()),
        ("Context", detail.context.as_str()),
        ("Why", detail.why.as_str()),
        ("Implications", detail.implications.as_str()),
        ("Alternatives", detail.alternatives.as_str()),
    ]
    .into_iter()
    .filter(|(_, text)| !text.trim().is_empty())
    .collect()
}

#[cfg(test)]
#[path = "tests/peek.rs"]
mod tests;
