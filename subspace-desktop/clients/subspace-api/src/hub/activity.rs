use super::{
    ActivitySection, ActivitySnapshot, Hub, HubError, ReadPriority, Shared, State, lock,
    worker::{Job, Task},
};
use crate::ActivityQuery;
use std::time::Instant;

impl Hub {
    pub fn activate_activity(&self, query: ActivityQuery) -> Result<(), HubError> {
        self.request_activity(query, false)
    }
    pub fn reload_activity(&self, query: ActivityQuery) -> Result<(), HubError> {
        self.request_activity(query, true)
    }
    fn request_activity(&self, query: ActivityQuery, force: bool) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        state.select_project(query.selected_project());
        if state.snapshot.me.is_none() {
            state.pending_activity = Some(query);
            return Ok(());
        }
        let needed = force
            || state.snapshot.activity.as_ref().is_none_or(|activity| {
                activity.query() != &query || activity.stale(Instant::now())
            });
        if needed {
            self.0.begin_activity(&mut state, query);
        }
        Ok(())
    }
    pub fn retry_activity(&self, section: ActivitySection) -> Result<(), HubError> {
        self.activity_page(section, true)
    }
    pub fn load_more_activity(&self, section: ActivitySection) -> Result<(), HubError> {
        self.activity_page(section, false)
    }
    fn activity_page(&self, section: ActivitySection, retry: bool) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(activity) = state.snapshot.activity.as_mut() else {
            return Ok(());
        };
        let query = activity.query().clone();
        let list = activity.section_mut(section);
        if list.loading || list.page_loading {
            return Ok(());
        }
        let cursor = if retry && list.error.is_some() {
            None
        } else {
            list.page.as_ref().and_then(|page| page.next_cursor.clone())
        };
        if !retry && cursor.is_none() {
            return Ok(());
        }
        list.begin(cursor.is_some());
        state.changed();
        let priority = if cursor.is_some() {
            ReadPriority::LaterPage
        } else {
            ReadPriority::Visible
        };
        self.0.enqueue(
            &mut state,
            Task::Activity {
                query,
                section,
                cursor,
            },
            priority,
        );
        Ok(())
    }
}

impl Shared {
    pub(super) fn begin_activity(&self, state: &mut State, query: ActivityQuery) {
        let changed = state
            .snapshot
            .activity
            .as_ref()
            .is_none_or(|activity| activity.query() != &query);
        if changed {
            state.snapshot.activity = Some(ActivitySnapshot::new(query.clone()));
        }
        for section in [ActivitySection::Trackers, ActivitySection::Recent] {
            let loading = state
                .snapshot
                .activity
                .as_ref()
                .is_some_and(|activity| activity.section(section).page_loading);
            if loading && !changed {
                if let Some(activity) = state.snapshot.activity.as_mut() {
                    activity.section_mut(section).head_pending = true;
                }
                continue;
            }
            state.activity_revision[section as usize] =
                state.activity_revision[section as usize].wrapping_add(1);
            if let Some(activity) = state.snapshot.activity.as_mut() {
                activity.section_mut(section).begin(false);
            }
            self.enqueue(
                state,
                Task::Activity {
                    query: query.clone(),
                    section,
                    cursor: None,
                },
                ReadPriority::Visible,
            );
        }
        state.changed();
    }
}

pub(super) fn load(
    shared: &Shared,
    job: &Job,
    request: (&ActivityQuery, ActivitySection, Option<&str>),
) {
    let (query, section, cursor) = request;
    let result = match section {
        ActivitySection::Trackers => shared.client.trackers_in_progress(query, cursor),
        ActivitySection::Recent => shared.client.activity_issues(query, cursor),
    };
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    let Some(activity) = state.snapshot.activity.as_mut() else {
        return;
    };
    let list = activity.section_mut(section);
    match result {
        Ok(page) => {
            let repeated = cursor.is_some() && page.next_cursor.as_deref() == cursor;
            if let Some(loaded) = list.page.as_mut() {
                super::state::activity::merge::merge(loaded, page, cursor.is_none());
            } else {
                list.page = Some(page);
            }
            list.loading = false;
            list.page_loading = false;
            list.updated_at = Some(Instant::now());
            if repeated {
                list.page_error = Some(super::ReadError::Response { status: None });
            }
        }
        Err(crate::Error::UnexpectedStatus(400)) if cursor.is_some() => {
            *list = Default::default();
            list.head_pending = true;
        }
        Err(error) => list.fail(cursor.is_some(), error.into()),
    }
    state.changed();
    finish_head(shared, &mut state, query, section);
}

fn finish_head(
    shared: &Shared,
    state: &mut State,
    query: &ActivityQuery,
    section: ActivitySection,
) {
    let pending = state
        .snapshot
        .activity
        .as_ref()
        .is_some_and(|activity| activity.section(section).head_pending);
    if pending {
        if let Some(activity) = state.snapshot.activity.as_mut() {
            activity.section_mut(section).begin(false);
        }
        shared.enqueue(
            state,
            Task::Activity {
                query: query.clone(),
                section,
                cursor: None,
            },
            ReadPriority::Visible,
        );
    }
}
