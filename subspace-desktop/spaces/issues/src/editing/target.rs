use crate::{Issues, model::ProjectScope};
use subspace_api_client::{hub::IssueStatus, model::IssueDetail};

pub(super) fn editable_detail(issues: &Issues) -> Option<&IssueDetail> {
    let open = issues.issue_view.snapshot.as_ref()?;
    let detail = open.detail()?;
    (*open.status() == IssueStatus::Loaded
        && detail.fields.archived_at.is_none()
        && issues
            .projects
            .iter()
            .any(|project| project.id == detail.fields.project_id && project.archived_at.is_none()))
    .then_some(detail)
}

pub(super) fn chosen_project(issues: &Issues) -> Option<String> {
    let mut active = issues
        .projects
        .iter()
        .filter(|project| project.archived_at.is_none());
    active
        .clone()
        .find(|project| issues.project == ProjectScope::Project(project.key.clone()))
        .or_else(|| active.next())
        .map(|project| project.id.clone())
}
