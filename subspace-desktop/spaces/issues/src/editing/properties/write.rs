use super::Field;
use subspace_api_client::issue_writes::{IssueChoice, IssueProperty};

pub(super) fn property(field: Field, values: &[IssueChoice]) -> Option<IssueProperty> {
    let id = values.first().map(|v| v.id().to_owned());
    Some(match field {
        Field::Project => return None,
        Field::State => IssueProperty::State(id?),
        Field::Priority => IssueProperty::Priority(id?.parse().ok()?),
        Field::Owner => IssueProperty::Owner(values.first()?.human_id()?.to_owned()),
        Field::Assignee => IssueProperty::Assignee(id),
        Field::Labels => IssueProperty::Labels(values.iter().map(|v| v.id().to_owned()).collect()),
        Field::Estimate => IssueProperty::Estimate(id.and_then(|v| v.parse().ok())),
        Field::Type => IssueProperty::Type(id?),
        Field::Milestone => IssueProperty::Milestone(id),
        Field::Cycle => IssueProperty::Cycle(id),
    })
}
