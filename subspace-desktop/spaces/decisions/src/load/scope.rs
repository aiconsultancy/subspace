//! Cursor-backed decision feeds. An initiative on Subspace reads its projects
//! separately and merges their heads in the same order as the API.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Arc;

use subspace_api_client::model::{DecisionListItem, Page, Project};
use subspace_api_client::{Backend, DecisionQuery, Error, MAX_DECISION_PAGE_SIZE, PageRequest};

use super::{Found, Plan};

const BATCH: usize = MAX_DECISION_PAGE_SIZE as usize;

type ReadPage<'a> =
    dyn FnMut(&DecisionQuery, &PageRequest) -> Result<Page<DecisionListItem>, Error> + 'a;

/// Where the next batch starts, including any already-fetched rows that did
/// not fit in the preceding batch.
#[derive(Clone)]
pub(crate) struct Continuation {
    feeds: Vec<Feed>,
    local_keys: Option<BTreeSet<String>>,
}

#[derive(Clone)]
struct Feed {
    project: Option<String>,
    next: Option<PageRequest>,
    seen: HashSet<String>,
    rows: VecDeque<DecisionListItem>,
}

impl Feed {
    fn new(project: Option<String>) -> Self {
        let mut request = PageRequest::default();
        request.limit = MAX_DECISION_PAGE_SIZE;
        Self {
            project,
            next: Some(request),
            seen: HashSet::new(),
            rows: VecDeque::new(),
        }
    }

    fn fill(
        &mut self,
        plan: &Plan,
        keys: Option<&BTreeSet<String>>,
        read: &mut ReadPage<'_>,
    ) -> Result<(), Error> {
        while self.rows.is_empty() && !plan.stop.stopped() {
            let Some(mut request) = self.next.take() else {
                break;
            };
            let mut query = plan.query.clone();
            if self.project.is_some() {
                query.project.clone_from(&self.project);
            }
            let page = read(&query, &request)?;
            if let Some(cursor) = page.next_cursor {
                if !self.seen.insert(cursor.clone()) {
                    return Err(Error::InvalidResponse);
                }
                request.cursor = Some(cursor);
                self.next = Some(request);
            }
            self.rows.extend(
                page.items
                    .into_iter()
                    .filter(|item| keys.is_none_or(|keys| keys.contains(&item.project_key))),
            );
        }
        Ok(())
    }

    fn has_more(&self) -> bool {
        self.next.is_some() || !self.rows.is_empty()
    }
}

impl Continuation {
    fn start(plan: &Plan, projects: &[Project], backend: Backend) -> Self {
        let mut feeds = vec![Feed::new(None)];
        let mut local_keys = None;
        if let Some(initiative) = plan.initiative.as_deref() {
            let keys = project_keys(initiative, plan.query.project.as_deref(), projects);
            if keys.is_empty() || backend != Backend::Nett {
                feeds = keys
                    .into_iter()
                    .map(|key| Feed::new(Some(key.to_owned())))
                    .collect();
            } else {
                // NETT's decisions endpoint has no project filter.
                local_keys = Some(keys.into_iter().map(str::to_owned).collect());
            }
        }
        Self { feeds, local_keys }
    }

    fn take_batch(mut self, plan: &Plan, read: &mut ReadPage<'_>) -> Result<Found, Error> {
        let mut items = Vec::with_capacity(BATCH);
        while items.len() < BATCH && !plan.stop.stopped() {
            for feed in &mut self.feeds {
                feed.fill(plan, self.local_keys.as_ref(), read)?;
            }
            let Some(feed) = newest(&mut self.feeds) else {
                break;
            };
            items.extend(feed.rows.pop_front());
        }
        let next = self
            .feeds
            .iter()
            .any(Feed::has_more)
            .then(|| Arc::new(self));
        Ok(Found {
            items,
            next,
            reveal: None,
        })
    }
}

fn newest(feeds: &mut [Feed]) -> Option<&mut Feed> {
    feeds
        .iter_mut()
        .filter(|feed| !feed.rows.is_empty())
        .max_by(|a, b| {
            a.rows
                .front()
                .map(order_key)
                .cmp(&b.rows.front().map(order_key))
        })
}

pub(super) fn read(
    plan: &Plan,
    projects: &[Project],
    backend: Backend,
    mut page: impl FnMut(&DecisionQuery, &PageRequest) -> Result<Page<DecisionListItem>, Error>,
) -> Result<Found, Error> {
    let continuation = plan
        .next
        .as_deref()
        .cloned()
        .unwrap_or_else(|| Continuation::start(plan, projects, backend));
    continuation.take_batch(plan, &mut page)
}

/// Find just one record in the existing initiative scope. A NETT
/// initiative filters locally and keeps full server pages to scan them.
pub(super) fn first(
    plan: &Plan,
    projects: &[Project],
    backend: Backend,
    read: &mut ReadPage<'_>,
) -> Result<Option<DecisionListItem>, Error> {
    let Continuation { feeds, local_keys } = Continuation::start(plan, projects, backend);
    for mut feed in feeds {
        if local_keys.is_none()
            && let Some(page) = feed.next.as_mut()
        {
            page.limit = 1;
        }
        feed.fill(plan, local_keys.as_ref(), read)?;
        if let Some(item) = feed.rows.pop_front() {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

fn order_key(item: &DecisionListItem) -> (&str, &str) {
    (&item.created_at, &item.id)
}

fn project_keys<'a>(
    initiative: &str,
    selected: Option<&str>,
    projects: &'a [Project],
) -> BTreeSet<&'a str> {
    let mut keys: BTreeSet<_> = projects
        .iter()
        .filter(|project| crate::initiative::includes(project, Some(initiative)))
        .map(|project| project.key.as_str())
        .collect();
    if let Some(selected) = selected.and_then(|key| keys.get(key).copied()) {
        keys.retain(|key| *key == selected);
    }
    keys
}

#[cfg(test)]
mod tests;
