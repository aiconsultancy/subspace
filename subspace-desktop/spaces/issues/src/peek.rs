//! What the inspector shows of the selected issue: a glance drawn from the
//! row the list already holds. It makes no read of its own, because the hub
//! holds one open issue at a time.

use subspace_api_client::model::{IssueListItem, Person, Project};

use crate::model::{IssuePriority, ProjectScope, StateKind};

/// The selected issue at a glance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Peek {
    pub(crate) identifier: String,
    pub(crate) state: String,
    pub(crate) state_kind: StateKind,
    pub(crate) title: String,
    pub(crate) summary: Option<String>,
    /// Priority, assignee and project always; labels and the due date when
    /// the issue has them.
    pub(crate) facts: Vec<(&'static str, String)>,
    /// The latest change with its actor and age, when the row carries one.
    pub(crate) activity: Option<String>,
}

/// The peek of `item`. `age` is how long ago its last activity was.
// qual:allow(dry, boilerplate) reason: "The peek copies the text it shows from a row the list keeps; the inspector must not borrow the board's listing while it renders."
pub(crate) fn peek(item: &IssueListItem, projects: &[Project], age: &str) -> Peek {
    let fields = &item.fields;
    Peek {
        identifier: fields.identifier.clone(),
        state: fields.state.name.clone(),
        state_kind: StateKind::parse(&fields.state.kind),
        title: fields.title.clone(),
        summary: fields
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .map(str::to_owned),
        facts: facts(item, projects),
        activity: fields
            .last_activity
            .as_ref()
            .map(|activity| crate::activity_words::summary(Some(activity), age)),
    }
}

fn facts(item: &IssueListItem, projects: &[Project]) -> Vec<(&'static str, String)> {
    let fields = &item.fields;
    let project = ProjectScope::Project(fields.project_key.clone());
    let mut facts = vec![
        (
            "Priority",
            IssuePriority::from_value(fields.priority)
                .look()
                .label
                .to_owned(),
        ),
        (
            "Assignee",
            fields
                .assignee
                .as_ref()
                .map_or_else(|| "Unassigned".to_owned(), Person::fact_label),
        ),
        (
            project_word(),
            crate::pickers::project_label(&project, projects),
        ),
    ];
    if !fields.labels.is_empty() {
        let names: Vec<_> = fields
            .labels
            .iter()
            .map(|label| label.name.as_str())
            .collect();
        facts.push(("Labels", names.join(", ")));
    }
    facts.extend(
        fields
            .due_date
            .as_deref()
            .map(|date| ("Due date", crate::view::issue::format_date(date))),
    );
    facts
}

/// In NETT the desktop's project is the issue team.
fn project_word() -> &'static str {
    if crate::pickers::teams() {
        "Team"
    } else {
        "Project"
    }
}

#[cfg(test)]
#[path = "tests/peek.rs"]
mod tests;
