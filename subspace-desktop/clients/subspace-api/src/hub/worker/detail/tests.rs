use super::{children, loaded, recheck::recheck, relations};
use crate::{
    IssueQuery, PageRequest,
    hub::{
        Hub, IssueSnapshot, IssueStatus, lock,
        worker::{Job, Task},
    },
    model::IssueDetail,
    tests::{common_path, connected, detail, page, row, wait},
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::Mock;

#[test]
fn refresh_keeps_a_known_milestone_name_only_while_its_id_is_unchanged() {
    let mut issue = IssueSnapshot::new("SUB-1", crate::PROFILE_ID, IssueStatus::Loading);
    let mut detail: IssueDetail = serde_json::from_value(crate::tests::detail("1")).unwrap();
    detail.fields.milestone_id = Some("milestone-1".into());
    loaded(&mut issue, detail.clone());
    assert!(issue.milestone_loading());
    issue.milestone_name = Some("First delivery".into());
    loaded(&mut issue, detail.clone());
    assert_eq!(issue.milestone_name(), Some("First delivery"));
    detail.fields.milestone_id = Some("milestone-2".into());
    loaded(&mut issue, detail);
    assert_eq!(issue.milestone_name(), None);
    let mut cleared: IssueDetail = serde_json::from_value(crate::tests::detail("1")).unwrap();
    cleared.fields.milestone_id = None;
    loaded(&mut issue, cleared);
    assert!(!issue.milestone_loading());
}

/// The open issue SUB-1: children 1–2 first, then 3–4 behind `c-1`; one
/// relation first, then R2 behind `r-1`. Moving it edits child 3, removes
/// child 4 and R2, and ends both chains early. The first pages never change
/// and always hand out the same cursors.
struct Served {
    moved: AtomicBool,
    fail: AtomicBool,
}

impl Served {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            moved: AtomicBool::new(false),
            fail: AtomicBool::new(false),
        })
    }

    fn moved(&self) -> bool {
        self.moved.load(Ordering::SeqCst)
    }

    fn child(&self, id: &str) -> Value {
        let mut value = row(id);
        if id == "3" && self.moved() {
            value["title"] = "Edited elsewhere".into();
        }
        value
    }

    fn relation(id: &str) -> Value {
        let mut value = row(id);
        value["kind"] = "blocks".into();
        value
    }

    fn detail(&self) -> Value {
        let mut value = detail("1");
        value["children"] =
            json!({"items":[self.child("11"), self.child("12")],"nextCursor":"c-1"});
        value["relations"] = json!({"items":[Self::relation("21")],"nextCursor":"r-1"});
        value
    }

    fn answer(&self, path: &str) -> (u16, String) {
        let bare = path.split('?').next().unwrap();
        if self.fail.load(Ordering::SeqCst) && path.contains("cursor=") {
            return (500, "{}".into());
        }
        match bare {
            "/api/v1/issues/SUB-1" | "/api/v1/issues/1" => (200, self.detail().to_string()),
            "/api/v1/issues" if path.contains("parent=") => {
                let rows = if self.moved() {
                    vec![self.child("3")]
                } else {
                    vec![self.child("3"), self.child("4")]
                };
                (200, page(rows, None))
            }
            "/api/v1/issues/SUB-1/relations" if path.contains("cursor=") => {
                let rows = if self.moved() {
                    vec![]
                } else {
                    vec![Self::relation("22")]
                };
                (200, page(rows, None))
            }
            _ => common_path(path),
        }
    }
}

fn opened(served: &Arc<Served>) -> (Mock, Hub) {
    let answer = served.clone();
    let mock = Mock::new(move |request| answer.answer(&request.path));
    let hub = connected(&mock);
    hub.load_more_subissues().unwrap();
    hub.load_more_relations().unwrap();
    wait(&hub, |s| {
        let issue = s.issue().unwrap();
        let detail = issue.detail().unwrap();
        detail.children.as_ref().unwrap().items.len() == 4
            && detail.relations.as_ref().unwrap().items.len() == 2
    });
    (mock, hub)
}

/// (child ids, child titles, child cursor, relation ids, relation cursor)
type Shown = (
    Vec<String>,
    Vec<String>,
    Option<String>,
    Vec<String>,
    Option<String>,
);

fn shown(hub: &Hub) -> Shown {
    let snapshot = hub.snapshot();
    let detail = snapshot.issue().unwrap().detail().unwrap();
    let children = detail.children.as_ref().unwrap();
    let relations = detail.relations.as_ref().unwrap();
    (
        children.items.iter().map(|r| r.fields.id.clone()).collect(),
        children
            .items
            .iter()
            .map(|r| r.fields.title.clone())
            .collect(),
        children.next_cursor.clone(),
        relations
            .items
            .iter()
            .map(|r| r.issue.fields.id.clone())
            .collect(),
        relations.next_cursor.clone(),
    )
}

fn reread(hub: &Hub) {
    let job = Job::new(&lock(&hub.0.state), Task::Recheck("SUB-1".into()));
    recheck(&hub.0, &job, "SUB-1");
}

#[test]
fn a_tail_child_edited_or_removed_below_an_unchanged_head_is_read_again() {
    let served = Served::new();
    let (_mock, hub) = opened(&served);
    // Both held cursors are used up: nothing else would read the tail.
    let (_, _, cursor, _, relation_cursor) = shown(&hub);
    assert_eq!((cursor, relation_cursor), (None, None));
    served.moved.store(true, Ordering::SeqCst);
    reread(&hub);
    let (ids, titles, cursor, relations, relation_cursor) = shown(&hub);
    assert_eq!(ids, ["11", "12", "3"], "child 4 removed");
    assert_eq!(titles[2], "Edited elsewhere");
    assert_eq!(cursor, None);
    assert_eq!(relations, ["21"], "relation 22 removed");
    assert_eq!(relation_cursor, None);
    hub.stop();
}

#[test]
fn a_failed_tail_read_keeps_what_is_shown() {
    let served = Served::new();
    let (_mock, hub) = opened(&served);
    let before = shown(&hub);
    served.moved.store(true, Ordering::SeqCst);
    served.fail.store(true, Ordering::SeqCst);
    reread(&hub);
    assert_eq!(shown(&hub), before);
    hub.stop();
}

#[test]
fn a_continuation_with_the_same_cursor_across_a_reread_is_dropped_with_its_error() {
    let served = Served::new();
    let answer = served.clone();
    let mock = Mock::new(move |request| answer.answer(&request.path));
    let hub = connected(&mock);
    let (children_epoch, relations_epoch) = {
        let state = lock(&hub.0.state);
        let epochs = state.snapshot.issue.as_ref().unwrap().epochs;
        (epochs.children, epochs.relations)
    };
    let mut query = IssueQuery::new("");
    query.parent = Some("1".into());
    query.page.cursor = Some("c-1".into());
    let request = PageRequest {
        cursor: Some("r-1".into()),
        ..PageRequest::default()
    };
    let job = Job::new(
        &lock(&hub.0.state),
        Task::Children(query.clone(), children_epoch),
    );
    // A re-read installs first pages that hand out the same cursors.
    reread(&hub);
    {
        let mut state = lock(&hub.0.state);
        let issue = state.snapshot.issue.as_mut().unwrap();
        issue.children_loading = true;
        issue.relations_loading = true;
    }
    children(&hub.0, &job, &query, children_epoch);
    relations(&hub.0, &job, ("SUB-1", &request, relations_epoch));
    let (ids, _, cursor, relation_ids, relation_cursor) = shown(&hub);
    assert_eq!(ids, ["11", "12"], "no stale rows appended");
    assert_eq!(cursor.as_deref(), Some("c-1"));
    assert_eq!(relation_ids, ["21"]);
    assert_eq!(relation_cursor.as_deref(), Some("r-1"));
    served.fail.store(true, Ordering::SeqCst);
    children(&hub.0, &job, &query, children_epoch);
    relations(&hub.0, &job, ("SUB-1", &request, relations_epoch));
    let snapshot = hub.snapshot();
    let issue = snapshot.issue().unwrap();
    assert!(
        !issue.children_failed() && !issue.relations_failed(),
        "stale errors dropped"
    );
    assert!(issue.children_loading() && issue.relations_loading());
    hub.stop();
}
