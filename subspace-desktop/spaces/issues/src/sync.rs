//! What the hub's snapshot means for the board and its Settings page. The
//! board reads a snapshot only when the hub's version changes, and never
//! drains the hub's events; everything here is pure.

use std::time::Instant;

use subspace_api_client::hub::{HubSnapshot, HubStatus, LiveStatus, ReadError};
use subspace_api_client::model::{IssueGroups, Project};

use crate::listing::{Failure, IssueGroup, ListId, More, Status};
use crate::model::StateKind;
use crate::scope::Access;

/// The board's state from one snapshot.
#[derive(Debug, Clone)]
pub(crate) struct HubView {
    pub(crate) profile_name: String,
    pub(crate) access: Access,
    pub(crate) projects: Vec<Project>,
    pub(crate) groups: Option<Vec<IssueGroup>>,
    pub(crate) list_project: Option<String>,
    pub(crate) read_at: Option<Instant>,
    /// Which grouped list the groups come from.
    pub(crate) list: Option<ListId>,
    pub(crate) request_revision: u64,
    pub(crate) status: Status,
    pub(crate) hidden_subissues_count: Option<u32>,
    pub(crate) my_issues_count: Option<u32>,
    pub(crate) live: LiveStatus,
}

pub(crate) fn view(snapshot: &HubSnapshot) -> HubView {
    let pages = |index| (snapshot.page_loading(index), snapshot.page_failed(index));
    HubView {
        profile_name: snapshot.profile_name().to_owned(),
        access: access(snapshot.status(), snapshot.me()),
        projects: snapshot.projects().to_vec(),
        groups: snapshot
            .list()
            .map(|list| groups(list, snapshot.projects(), pages)),
        list_project: snapshot.list_project().map(str::to_owned),
        read_at: snapshot.updated_at(),
        list: snapshot.list_id().map(|installation| ListId {
            generation: snapshot.generation(),
            installation,
        }),
        request_revision: snapshot.list_revision(),
        status: list_status(snapshot.status(), snapshot.list_loading()),
        hidden_subissues_count: snapshot.hidden_subissues_count(),
        my_issues_count: snapshot.my_issues_count(),
        live: snapshot.live(),
    }
}

/// The board's words for a refused or failed read. Nothing from a response
/// body reaches them.
pub(crate) fn failure(error: &ReadError) -> Failure {
    match error {
        ReadError::SignInNeeded => Failure::Edge,
        ReadError::Forbidden => Failure::Forbidden,
        ReadError::Network => Failure::Network,
        ReadError::Response { status } => Failure::Unexpected { status: *status },
        ReadError::OrganisationNeeded => Failure::Organisation,
    }
}

pub(crate) fn access(status: &HubStatus, me: Option<&subspace_api_client::model::Me>) -> Access {
    match status {
        HubStatus::Error(error) if me.is_some() && !failure(error).is_account_level() => {
            Access::Ready {
                display_name: me.map(|me| me.display_name.clone()).unwrap_or_default(),
            }
        }
        HubStatus::Error(error) => Access::Failed(failure(error)),
        HubStatus::Stopped => Access::Failed(Failure::Offline),
        _ => me.map_or(Access::Starting, |me| Access::Ready {
            display_name: me.display_name.clone(),
        }),
    }
}

/// The list's read status: reading, a read error that keeps the list in
/// view, or settled.
pub(crate) fn list_status(status: &HubStatus, loading: bool) -> Status {
    if loading {
        return Status::Loading;
    }
    match status {
        HubStatus::Error(error) => Status::Failed(failure(error)),
        _ => Status::Idle,
    }
}

/// The hub's groups as the board draws them. A project's group takes its
/// workflow state's name; a state type's takes Subspace API's default name.
pub(crate) fn groups(
    list: &IssueGroups,
    projects: &[Project],
    pages: impl Fn(usize) -> (bool, bool),
) -> Vec<IssueGroup> {
    list.groups
        .iter()
        .enumerate()
        .map(|(index, group)| IssueGroup {
            index,
            key: group
                .state_id
                .clone()
                .unwrap_or_else(|| group.state_type.clone()),
            name: group_name(group, projects),
            kind: StateKind::parse(&group.state_type),
            total: Some(group.count),
            rows: group.items.clone(),
            more: more(group, pages(index)),
        })
        .collect()
}

fn group_name(group: &subspace_api_client::model::IssueGroup, projects: &[Project]) -> String {
    let workflow = group.state_id.as_deref().and_then(|id| {
        projects
            .iter()
            .flat_map(|project| &project.states)
            .find(|state| state.id == id)
            .map(|state| state.name.clone())
    });
    let shown = group
        .items
        .first()
        .map(|item| item.fields.state.name.clone());
    workflow
        .or(shown)
        .unwrap_or_else(|| StateKind::parse(&group.state_type).label().to_owned())
}

/// What lies past a group's loaded rows, as the hub's `load_more` reads it.
pub(crate) fn more(
    group: &subspace_api_client::model::IssueGroup,
    (loading, failed): (bool, bool),
) -> More {
    let loaded = group.items.len();
    let all_loaded = loaded >= group.count as usize;
    match (
        loading,
        failed,
        all_loaded,
        loaded == 0,
        group.next_cursor.is_some(),
    ) {
        (true, ..) => More::Loading,
        (_, true, ..) => More::Failed,
        (_, _, true, ..) => More::None,
        (_, _, _, true, _) => More::Unread,
        (_, _, _, _, true) => More::Available,
        _ => More::None,
    }
}

#[cfg(test)]
#[path = "tests/sync.rs"]
mod tests;
