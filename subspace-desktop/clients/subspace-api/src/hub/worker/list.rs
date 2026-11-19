use super::{
    Job, Shared, Task, lock,
    pages::{Extent, extend_unique, fill},
};
use crate::{
    Error, ListQuery,
    hub::{HubStatus, ReadPriority},
    model::IssueGroup,
};
use std::time::Instant;

pub(super) fn load(shared: &Shared, job: &Job, query: &ListQuery) {
    let result = read_list(shared, job, query);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    match result {
        Ok(groups) => state.install_list(groups),
        Err(error) => {
            state.snapshot.list_loading = false;
            state.snapshot.status = HubStatus::Error(error.into());
        }
    }
    state.changed();
    drop(state);
    counts(shared, job, query);
    let mut state = lock(&shared.state);
    if job.current(&state)
        && state.live_reads.list_after_load.take() == Some((job.generation, job.revision))
    {
        crate::hub::live::reread_list(shared, &mut state);
    }
}

/// A live hint's read. The fresh first pages are followed with their own
/// cursors as far as each group was paged in, then replace the groups whole
/// as a new installation, so every shown row is read again and none is kept
/// from an older read. The shown rows stay on screen until then. The counts
/// are read again in the same spaced, coalesced read. A failed read leaves
/// the list as it is.
pub(super) fn head(shared: &Shared, job: &Job, query: &ListQuery) {
    let shown = Shown::of(&lock(&shared.state));
    let result = shared
        .client
        .issue_groups(query)
        .and_then(|fresh| repage(shared, query, fresh, &shown));
    let read = result.is_ok();
    let mut state = lock(&shared.state);
    if let Ok(fresh) = result {
        take_head(job, &mut state, query, fresh, &shown);
    }
    drop(state);
    if read {
        counts(shared, job, query);
    }
    land_head(shared, job, &mut lock(&shared.state));
}

/// The installation a head read started from, and each group's shown rows.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Shown {
    list_id: u64,
    rows: Vec<(Option<String>, String, usize)>,
}

impl Shown {
    pub(super) fn of(state: &crate::hub::State) -> Self {
        let groups = state.snapshot.list.iter().flat_map(|list| &list.groups);
        Self {
            list_id: state.snapshot.list_id,
            rows: groups
                .map(|group| {
                    (
                        group.state_id.clone(),
                        group.state_type.clone(),
                        group.items.len(),
                    )
                })
                .collect(),
        }
    }

    /// The rows shown for the group with this identity; none for a new one.
    fn rows(&self, group: &IssueGroup) -> usize {
        self.rows
            .iter()
            .find(|(id, kind, _)| *id == group.state_id && *kind == group.state_type)
            .map_or(0, |(_, _, rows)| *rows)
    }
}

/// Pages each fresh group from its own cursor up to the rows shown before,
/// a bounded number of later pages; any failed page fails the read.
fn repage(
    shared: &Shared,
    query: &ListQuery,
    mut fresh: crate::model::IssueGroups,
    shown: &Shown,
) -> Result<crate::model::IssueGroups, Error> {
    for group in &mut fresh.groups {
        let extent = Extent {
            rows: shown.rows(group),
            size: crate::list::LATER_PAGE as usize,
        };
        let probe = IssueGroup {
            items: Vec::new(),
            ..group.clone()
        };
        // Terminal groups in the group summary are counts-only. If this group
        // was expanded, start with its standalone first page and fresh cursor.
        if extent.rows > 0
            && group.count > 0
            && group.items.is_empty()
            && group.next_cursor.is_none()
        {
            let page = shared.client.issue_group_page(query, &probe, None)?;
            group.items = page.items;
            group.next_cursor = page.next_cursor;
        }
        let next = |cursor: &str| shared.client.issue_group_page(query, &probe, Some(cursor));
        fill(
            &mut group.items,
            &mut group.next_cursor,
            extent,
            next,
            |row| &row.fields.id,
        )?;
    }
    Ok(fresh)
}

pub(super) fn take_head(
    job: &Job,
    state: &mut crate::hub::State,
    query: &ListQuery,
    fresh: crate::model::IssueGroups,
    shown: &Shown,
) {
    if !job.current(state) || state.query.as_ref() != Some(query) || state.snapshot.list_loading {
        return;
    }
    if state.snapshot.list.is_none() {
        return;
    }
    if Shown::of(state) != *shown {
        // Paged further while this read was out: it no longer covers what
        // is shown, so it is read again rather than installed.
        state.live_reads.list.owe();
        return;
    }
    state.snapshot.list = Some(fresh);
    // A new installation: continuations asked for before it are dropped
    // when they land, so their groups can be paged again at once.
    // Unlike `install_list`, the hub's status is left alone: a background
    // read must not clear an error another read reported.
    state.snapshot.list_id = state.snapshot.list_id.wrapping_add(1);
    state.snapshot.page_loading.clear();
    state.snapshot.page_errors.clear();
    state.snapshot.updated_at = Some(Instant::now());
    state.changed();
}

/// The read is back, whatever came of it: hints that arrived meanwhile owe
/// one trailing read.
fn land_head(shared: &Shared, job: &Job, state: &mut crate::hub::State) {
    if state.live_reads.list.land((job.generation, job.revision)) && job.current(state) {
        crate::hub::live::reread_list(shared, state);
    }
}

fn read_list(
    shared: &Shared,
    job: &Job,
    query: &ListQuery,
) -> Result<crate::model::IssueGroups, crate::Error> {
    if let Some(project) = query.selected_project() {
        super::connection::ensure_states(shared, job, project)?;
    }
    shared.client.issue_groups(query)
}

pub(super) fn counts(shared: &Shared, job: &Job, query: &ListQuery) {
    if !job.current(&lock(&shared.state)) {
        return;
    }
    let hidden = query
        .hidden_count_parameters()
        .ok()
        .flatten()
        .and_then(|params| shared.client.count_groups(&params).ok())
        .map(|groups| groups.groups.iter().map(|group| group.count).sum::<u32>());
    let mine = shared
        .client
        .count_groups(&ListQuery::my_count_parameters())
        .ok()
        .map(|groups| groups.groups.iter().map(|group| group.count).sum::<u32>());
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    let shown = state
        .snapshot
        .list
        .as_ref()
        .map(|groups| groups.groups.iter().map(|group| group.count).sum::<u32>());
    state.snapshot.hidden_subissues_count = hidden
        .zip(shown)
        .map(|(all, shown)| all.saturating_sub(shown));
    state.snapshot.my_issues_count = mine;
    state.changed();
}

/// A group's next page, for the installation `list` it was asked for. A
/// reply or error for a replaced installation touches nothing: its loading,
/// error and cursor state belong to the new one.
pub(super) fn page(shared: &Shared, job: &Job, request: (&ListQuery, usize, &IssueGroup, u64)) {
    let (query, group, selected, list) = request;
    let result = shared
        .client
        .issue_group_page(query, selected, selected.next_cursor.as_deref());
    let mut state = lock(&shared.state);
    if !job.current(&state) || state.snapshot.list_id != list {
        return;
    }
    state.snapshot.page_loading.retain(|index| *index != group);
    if matches!(result, Err(Error::UnexpectedStatus(400))) && selected.next_cursor.is_some() {
        // A rejected cursor invalidates the list installation, including
        // first-page completion markers held by its readers.
        state.revision = state.revision.wrapping_add(1);
        state.snapshot.list_loading = true;
        state.snapshot.page_loading.clear();
        state.snapshot.page_errors.clear();
        state.changed();
        shared.enqueue(&mut state, Task::List(query.clone()), ReadPriority::Visible);
        return;
    }
    match result {
        Ok(page) => {
            if let Some(target) = state
                .snapshot
                .list
                .as_mut()
                .and_then(|list| list.groups.get_mut(group))
            {
                extend_unique(&mut target.items, page.items, |row| &row.fields.id);
                target.next_cursor = page.next_cursor;
            }
        }
        Err(_) => state.snapshot.page_errors.push(group),
    }
    state.changed();
}

#[cfg(test)]
mod tests;
