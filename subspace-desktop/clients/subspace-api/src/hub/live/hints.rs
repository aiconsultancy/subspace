//! What a hint touches in the shown state, and the reads it leads to.
use super::{Shared, Task, enqueue};
use crate::{hub::IssueStatus, hub::State, live::IssueChanged};
use std::{collections::BTreeSet, time::Instant};

/// The selected project and the open issue's project. All projects also
/// follows the catalogue so a project can send hints before it has any rows.
pub(super) fn wanted(state: &State) -> BTreeSet<String> {
    let open = state
        .snapshot
        .issue
        .as_ref()
        .and_then(|issue| issue.detail.as_ref())
        .map(|detail| detail.fields.project_key.clone());
    let shown: BTreeSet<String> = match &state.snapshot.selected_project {
        Some(project) => BTreeSet::from([project.clone()]),
        None => state
            .snapshot
            .list
            .iter()
            .flat_map(|list| &list.groups)
            .flat_map(|group| &group.items)
            .map(|row| row.fields.project_key.clone())
            .collect(),
    };
    let catalog = state
        .snapshot
        .projects
        .iter()
        .filter(|_| state.query.is_some() && state.snapshot.selected_project.is_none())
        .map(|project| project.key.clone());
    shown
        .into_iter()
        .chain(open)
        .chain(catalog)
        .filter(|key| !archived_project(state, key))
        .collect()
}

fn archived_project(state: &State, key: &str) -> bool {
    state
        .snapshot
        .projects
        .iter()
        .any(|project| project.key.eq_ignore_ascii_case(key) && project.archived_at.is_some())
}

/// Whether the hint touches the open issue and the shown list. Collaboration
/// writes (comments, threads, attachments, links, summaries, documents and
/// reviews) keep the issue's version, so a hint is never skipped by version:
/// replies older than what is held are dropped when they land instead.
pub(super) fn affects(state: &State, event: &IssueChanged) -> (bool, bool) {
    (open_issue(state, event), list(state, event))
}

fn same(id: &str, identifier: &str, event: &IssueChanged) -> bool {
    id == event.issue_id || identifier.eq_ignore_ascii_case(&event.identifier)
}

fn open_issue(state: &State, event: &IssueChanged) -> bool {
    let Some(issue) = state
        .snapshot
        .issue
        .as_ref()
        .filter(|issue| issue.profile_id == crate::PROFILE_ID)
    else {
        return false;
    };
    let (id, identifier) = issue
        .detail
        .as_ref()
        .map(|detail| (detail.fields.id.as_str(), detail.fields.identifier.as_str()))
        .unwrap_or((&issue.identifier, &issue.identifier));
    let parent = event
        .parent_identifier
        .as_deref()
        .is_some_and(|parent| parent.eq_ignore_ascii_case(identifier));
    parent || same(id, identifier, event) || shown_child(issue, event)
}

fn shown_child(issue: &crate::hub::IssueSnapshot, event: &IssueChanged) -> bool {
    issue
        .detail
        .as_ref()
        .and_then(|detail| detail.children.as_ref())
        .is_some_and(|children| {
            children
                .items
                .iter()
                .any(|row| same(&row.fields.id, &row.fields.identifier, event))
        })
}

fn list(state: &State, event: &IssueChanged) -> bool {
    if state.query.is_none() {
        return false;
    }
    let mut rows = state
        .snapshot
        .list
        .iter()
        .flat_map(|list| &list.groups)
        .flat_map(|group| &group.items);
    rows.any(|row| same(&row.fields.id, &row.fields.identifier, event))
        || state
            .snapshot
            .selected_project
            .as_deref()
            .is_none_or(|project| project.eq_ignore_ascii_case(&event.project_key))
}

/// A ready event or hint can arrive before the first detail has landed.
pub(super) fn update_issue(shared: &Shared, state: &mut State) {
    if state.snapshot.issue.as_ref().is_some_and(|issue| {
        issue.profile_id == crate::PROFILE_ID && issue.status == IssueStatus::Loading
    }) {
        state.live_reads.issue_after_load = Some(state.issue_key());
    } else {
        reread_issue(shared, state);
    }
}

/// Reads the open issue again when it is loaded, keeping what is shown.
/// While a read is out, one trailing read is owed instead.
pub(in crate::hub) fn reread_issue(shared: &Shared, state: &mut State) {
    let identifier = state
        .snapshot
        .issue
        .as_ref()
        .filter(|issue| {
            issue.profile_id == crate::PROFILE_ID && issue.status == IssueStatus::Loaded
        })
        .map(|issue| issue.identifier.clone());
    let key = state.issue_key();
    if let Some(identifier) = identifier
        && state.live_reads.issue.begin(key, Instant::now())
    {
        enqueue(shared, state, Task::Recheck(identifier));
    }
}

/// Reads the list head again, keeping the rows paged in below it.
/// While a read is out, one trailing read is owed instead.
pub(in crate::hub) fn reread_list(shared: &Shared, state: &mut State) {
    let key = state.list_key();
    if state.snapshot.list_loading {
        state.live_reads.list_after_load = Some(key);
        return;
    }
    if state.snapshot.list.is_none() {
        return;
    }
    if let Some(query) = state.query.clone()
        && state.live_reads.list.begin(key, Instant::now())
    {
        enqueue(shared, state, Task::ListHead(query));
    }
}

#[cfg(test)]
mod tests {
    use super::{open_issue, wanted};
    use crate::hub::{IssueSnapshot, IssueStatus, State};
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn the_open_issues_project_is_joined_beside_the_selected_one() {
        let mut state = State::new("http://127.0.0.1/api", "Fixture");
        state.snapshot.selected_project = Some("OPS".into());
        assert_eq!(wanted(&state), BTreeSet::from(["OPS".to_owned()]));
        let mut value = crate::tests::detail("1");
        value["projectKey"] = "SUB".into();
        let mut issue = IssueSnapshot::new("SUB-1", crate::PROFILE_ID, IssueStatus::Loaded);
        issue.detail = Some(serde_json::from_value(value).unwrap());
        state.snapshot.issue = Some(issue);
        assert_eq!(
            wanted(&state),
            BTreeSet::from(["OPS".to_owned(), "SUB".to_owned()])
        );
    }

    #[test]
    fn a_reparented_child_hint_touches_the_previous_tracker_when_its_row_is_shown() {
        let mut state = State::new("http://127.0.0.1/api", "Fixture");
        let mut detail = crate::tests::detail("1");
        detail["children"]["items"] = json!([crate::tests::row("2")]);
        let mut tracker = IssueSnapshot::new("SUB-1", crate::PROFILE_ID, IssueStatus::Loaded);
        tracker.detail = Some(serde_json::from_value(detail).unwrap());
        state.snapshot.issue = Some(tracker);
        let event = serde_json::from_value(json!({
            "projectKey":"OPS", "issueId":"2", "identifier":"OPS-4",
            "parentIdentifier":"OPS-2", "version":4,
        }))
        .unwrap();
        assert!(open_issue(&state, &event));
    }
}
