use serde::Serialize;

use super::{DESCRIPTION_LIMIT, IssueSaveError, TITLE_LIMIT};
const MAX_PRIORITY: i32 = 4;
const MIN_ESTIMATE: f64 = 0.;
const MAX_LABELS: usize = 100;

/// A value the issue writer may change. A nullable value set to `None`
/// clears that property. A property left out keeps the server value.
pub enum IssueProperty {
    Title(String),
    Summary(String),
    Description(Option<String>),
    State(String),
    Assignee(Option<String>),
    Priority(i32),
    DueDate(Option<String>),
    Labels(Vec<String>),
    Estimate(Option<f64>),
    Type(String),
    Parent(Option<String>),
    Milestone(Option<String>),
    Cycle(Option<String>),
}

/// Immutable once a save has started. Sparse PATCH keeps fields the person
/// did not change out of the body. POST uses the same editable fields.
// qual:allow(srp, god_struct) reason: "This flat wire shape mirrors the service's sparse issue fields and keeps absent and null distinct."
#[derive(Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueChanges {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    estimate: Option<Option<f64>>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    milestone_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cycle_id: Option<Option<String>>,
}

impl IssueChanges {
    pub fn set(&mut self, field: IssueProperty) {
        match field {
            IssueProperty::Title(title) => self.title = Some(title.trim().to_owned()),
            IssueProperty::Summary(text) => self.summary = Some(text.trim().to_owned()),
            IssueProperty::Description(text) => self.description = Some(text),
            IssueProperty::State(id) => self.state_id = Some(id),
            IssueProperty::Assignee(id) => self.assignee_id = Some(id),
            IssueProperty::Priority(priority) => self.priority = Some(priority),
            IssueProperty::DueDate(date) => self.due_date = Some(date),
            IssueProperty::Labels(ids) => self.label_ids = Some(ids),
            IssueProperty::Estimate(value) => self.estimate = Some(value),
            context => self.set_context(context),
        }
    }

    fn set_context(&mut self, property: IssueProperty) {
        match property {
            IssueProperty::Type(kind) => self.kind = Some(kind),
            IssueProperty::Parent(id) => self.parent_id = Some(id),
            IssueProperty::Milestone(id) => self.milestone_id = Some(id),
            IssueProperty::Cycle(id) => self.cycle_id = Some(id),
            _ => {}
        }
    }

    pub fn validate(&self, create: bool) -> Result<(), IssueSaveError> {
        if self.title.as_deref().map_or(create, invalid_title) {
            return Err(IssueSaveError::Title);
        }
        if self
            .description
            .as_ref()
            .and_then(Option::as_deref)
            .is_some_and(invalid_description)
        {
            return Err(IssueSaveError::Description);
        }
        if self
            .due_date
            .as_ref()
            .and_then(Option::as_deref)
            .is_some_and(invalid_date)
        {
            return Err(IssueSaveError::DueDate);
        }
        if self
            .summary
            .as_deref()
            .map_or(create, |text| !super::valid_summary(text))
        {
            return Err(IssueSaveError::Summary);
        }
        self.validate_properties()?;
        if self == &Self::default() {
            return Err(IssueSaveError::NoChanges);
        }
        Ok(())
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn invalid_title(title: &str) -> bool {
    title.is_empty()
        || title.contains(['\r', '\n', '\0'])
        || title.encode_utf16().count() > TITLE_LIMIT
}

fn invalid_description(description: &str) -> bool {
    description.contains('\0') || description.encode_utf16().count() > DESCRIPTION_LIMIT
}

fn invalid_date(date: &str) -> bool {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() || date.len() != "YYYY-MM-DD".len()
}

impl IssueChanges {
    fn validate_properties(&self) -> Result<(), IssueSaveError> {
        if self
            .priority
            .is_some_and(|p| !(0..=MAX_PRIORITY).contains(&p))
            || self
                .estimate
                .flatten()
                .is_some_and(|v| !v.is_finite() || v < MIN_ESTIMATE)
            || self
                .kind
                .as_deref()
                .is_some_and(|k| !matches!(k, "issue" | "tracker"))
            || self
                .label_ids
                .as_ref()
                .is_some_and(|ids| ids.len() > MAX_LABELS)
        {
            return Err(IssueSaveError::Invalid);
        }
        Ok(())
    }

    pub(crate) fn is_tracker(&self, current: bool) -> bool {
        self.kind.as_ref().map_or(current, |kind| kind == "tracker")
    }

    pub(crate) fn tracker_properties(&self) -> bool {
        self.state_id.is_some()
            || self.estimate.is_some()
            || self.cycle_id.as_ref().is_some_and(Option::is_some)
    }
}
