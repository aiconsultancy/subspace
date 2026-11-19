//! A live head read pages the fresh first pages again, from their own
//! cursors, as far as the shown groups reach, then installs them whole.
//! Continuations belong to the installation they were asked for.
mod terminal;

use super::{Shown, head, page, take_head};
use crate::{
    ListQuery,
    hub::{
        Hub, State, lock,
        worker::{Job, Task},
    },
    model::IssueGroups,
    tests::{common_path, connected, row, wait},
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::Mock;

fn group(state: &str, ids: &[&str], cursor: Option<&str>) -> Value {
    let items: Vec<Value> = ids.iter().map(|id| row(id)).collect();
    json!({"stateId":state,"stateType":"started","count":ids.len(),"items":items,
        "nextCursor":cursor})
}

fn groups(groups: Vec<Value>) -> IssueGroups {
    serde_json::from_value(json!({"groupBy":"state","groups":groups})).unwrap()
}

/// The held list, a head read of `fresh` landing on it, and the result as
/// `(state id, row ids, cursor)` per group.
fn land(held: IssueGroups, fresh: IssueGroups) -> Vec<(String, Vec<String>, Option<String>)> {
    let query = ListQuery::default();
    let mut state = State::new("http://127.0.0.1/api", "Fixture");
    state.query = Some(query.clone());
    state.snapshot.list = Some(held);
    let before = state.snapshot.list_id;
    let shown = Shown::of(&state);
    let job = Job::new(&state, Task::ListHead(query.clone()));
    take_head(&job, &mut state, &query, fresh, &shown);
    assert_ne!(state.snapshot.list_id, before, "a new installation");
    let list = state.snapshot.list.unwrap();
    list.groups
        .into_iter()
        .map(|group| {
            (
                group.state_id.unwrap_or_default(),
                group.items.into_iter().map(|row| row.fields.id).collect(),
                group.next_cursor,
            )
        })
        .collect()
}

fn ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn a_reorder_follows_the_fresh_order() {
    let held = groups(vec![group("s", &["1", "2", "3"], Some("old"))]);
    let fresh = groups(vec![group("s", &["3", "1", "2"], Some("new"))]);
    let out = land(held, fresh);
    assert_eq!(out[0].1, ids(&["3", "1", "2"]));
    assert_eq!(out[0].2.as_deref(), Some("new"));
}

#[test]
fn a_row_moved_to_another_group_shows_once() {
    let held = groups(vec![
        group("a", &["1", "2", "3"], None),
        group("b", &["7"], None),
    ]);
    let fresh = groups(vec![
        group("a", &["1"], Some("c-a")),
        group("b", &["3", "7"], None),
    ]);
    let out = land(held, fresh);
    assert_eq!(out[0].1, ids(&["1"]));
    assert_eq!(out[0].2.as_deref(), Some("c-a"));
    assert_eq!(out[1].1, ids(&["3", "7"]));
}

#[test]
fn a_read_that_no_longer_covers_the_shown_rows_is_read_again() {
    let query = ListQuery::default();
    let mut state = State::new("http://127.0.0.1/api", "Fixture");
    state.query = Some(query.clone());
    state.snapshot.list = Some(groups(vec![group("s", &["1"], Some("c"))]));
    let shown = Shown::of(&state);
    let job = Job::new(&state, Task::ListHead(query.clone()));
    let key = (job.generation, job.revision);
    assert!(state.live_reads.list.begin(key, std::time::Instant::now()));
    // The person paged in a row while the read was out.
    state.snapshot.list = Some(groups(vec![group("s", &["1", "2"], None)]));
    let before = state.snapshot.list_id;
    let fresh = groups(vec![group("s", &["1"], Some("c"))]);
    take_head(&job, &mut state, &query, fresh, &shown);
    assert_eq!(state.snapshot.list_id, before, "not installed");
    assert_eq!(
        state.snapshot.list.as_ref().unwrap().groups[0].items.len(),
        2
    );
    assert!(state.live_reads.list.land(key), "a trailing read is owed");
}

/// The list the loopback API serves: rows 1–2 first, then 3–4 behind
/// `c-1` and 5 behind `c-2`. Moving it edits row 3 and removes row 4.
struct Served {
    moved: AtomicBool,
    fail_pages: AtomicBool,
    /// The cursor the first page hands out.
    cursor: Mutex<String>,
}

impl Served {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            moved: AtomicBool::new(false),
            fail_pages: AtomicBool::new(false),
            cursor: Mutex::new("c-1".into()),
        })
    }

    fn row(&self, id: &str) -> Value {
        let mut value = row(id);
        if id == "3" && self.moved.load(Ordering::SeqCst) {
            value["title"] = "Edited elsewhere".into();
        }
        value
    }

    fn rows(&self, ids: &[&str]) -> Vec<Value> {
        let moved = self.moved.load(Ordering::SeqCst);
        ids.iter()
            .filter(|id| !(moved && **id == "4"))
            .map(|id| self.row(id))
            .collect()
    }

    fn answer(&self, path: &str) -> (u16, String) {
        if path.starts_with("/api/v1/issues/groups") && !path.contains("limitPerGroup=0") {
            let cursor = self.cursor.lock().unwrap().clone();
            let first = json!({"stateId":"state-1","stateType":"started","count":5,
                "items":self.rows(&["1", "2"]),"nextCursor":cursor});
            return (200, json!({"groupBy":"state","groups":[first]}).to_string());
        }
        if path.starts_with("/api/v1/issues?") && path.contains("stateId=") {
            if self.fail_pages.load(Ordering::SeqCst) {
                return (500, "{}".into());
            }
            let (ids, next): (&[&str], _) = if path.contains("cursor=c-2") {
                (&["5"], None)
            } else if path.contains("cursor=same") {
                (&["stale"], None)
            } else {
                (&["3", "4"], Some("c-2"))
            };
            return (200, crate::tests::page(self.rows(ids), next));
        }
        common_path(path)
    }
}

fn listed(served: &Arc<Served>) -> (Mock, Hub, ListQuery) {
    let answer = served.clone();
    let mock = Mock::new(move |request| answer.answer(&request.path));
    let hub = connected(&mock);
    let query = ListQuery::default().project(Some("SUB"));
    hub.activate(query.clone()).unwrap();
    wait(&hub, |s| s.list().is_some() && !s.list_loading());
    (mock, hub, query)
}

fn shown(hub: &Hub) -> (Vec<String>, Vec<String>, Option<String>) {
    let snapshot = hub.snapshot();
    let group = &snapshot.list().unwrap().groups[0];
    (
        group
            .items
            .iter()
            .map(|row| row.fields.id.clone())
            .collect(),
        group
            .items
            .iter()
            .map(|row| row.fields.title.clone())
            .collect(),
        group.next_cursor.clone(),
    )
}

fn reread(hub: &Hub, query: &ListQuery) {
    let job = Job::new(&lock(&hub.0.state), Task::ListHead(query.clone()));
    head(&hub.0, &job, query);
}

/// Pages the first group past its first page; the installation shown.
fn paged(hub: &Hub) -> Option<u64> {
    hub.load_more(0).unwrap();
    wait(hub, |s| s.list().unwrap().groups[0].items.len() == 4);
    hub.snapshot().list_id()
}

#[test]
fn a_reread_pages_the_shown_rows_again_from_fresh_cursors() {
    let served = Served::new();
    let (mock, hub, query) = listed(&served);
    let installation = paged(&hub);
    // Row 3 edited and row 4 removed elsewhere; the first page is the same.
    served.moved.store(true, Ordering::SeqCst);
    let pages = |mock: &Mock| {
        let requests = mock.requests();
        requests
            .iter()
            .filter(|r| r.path.starts_with("/api/v1/issues?"))
            .count()
    };
    let before = pages(&mock);
    reread(&hub, &query);
    let (rows, titles, cursor) = shown(&hub);
    assert_eq!(rows, ids(&["1", "2", "3"]), "row 4 is gone");
    assert_eq!(titles[2], "Edited elsewhere");
    // One later page was shown, so one is read again; row 5 pages in next,
    // from the fresh chain's own cursor.
    assert_eq!(cursor.as_deref(), Some("c-2"));
    assert_eq!(pages(&mock) - before, 1);
    assert_ne!(hub.snapshot().list_id(), installation);
    hub.stop();
}

#[test]
fn a_failed_page_during_a_reread_keeps_what_is_shown() {
    let served = Served::new();
    let (_mock, hub, query) = listed(&served);
    let installation = paged(&hub);
    served.moved.store(true, Ordering::SeqCst);
    served.fail_pages.store(true, Ordering::SeqCst);
    reread(&hub, &query);
    let (rows, titles, cursor) = shown(&hub);
    assert_eq!(rows, ids(&["1", "2", "3", "4"]));
    assert_ne!(titles[2], "Edited elsewhere");
    assert_eq!(cursor.as_deref(), Some("c-2"));
    assert_eq!(hub.snapshot().list_id(), installation);
    hub.stop();
}

#[test]
fn a_continuation_with_the_same_cursor_across_a_reread_is_dropped_with_its_error() {
    let served = Served::new();
    *served.cursor.lock().unwrap() = "same".into();
    let (_mock, hub, query) = listed(&served);
    let (old, selected) = {
        let state = lock(&hub.0.state);
        let mut selected = state.snapshot.list.as_ref().unwrap().groups[0].clone();
        selected.items.clear();
        (state.snapshot.list_id, selected)
    };
    let stale = Job::new(
        &lock(&hub.0.state),
        Task::Page {
            query: query.clone(),
            group: 0,
            selected: selected.clone(),
            list: old,
        },
    );
    // A re-read installs a list whose first page hands out the same cursor.
    reread(&hub, &query);
    assert_ne!(hub.snapshot().list_id(), Some(old));
    lock(&hub.0.state).snapshot.page_loading.push(0);
    page(&hub.0, &stale, (&query, 0, &selected, old));
    let (rows, _, cursor) = shown(&hub);
    assert_eq!(rows, ids(&["1", "2"]), "no stale rows appended");
    assert_eq!(cursor.as_deref(), Some("same"));
    assert!(
        hub.snapshot().page_loading(0),
        "loading belongs to the new list"
    );
    served.fail_pages.store(true, Ordering::SeqCst);
    page(&hub.0, &stale, (&query, 0, &selected, old));
    assert!(!hub.snapshot().page_failed(0), "a stale error is dropped");
    assert!(hub.snapshot().page_loading(0));
    hub.stop();
}
