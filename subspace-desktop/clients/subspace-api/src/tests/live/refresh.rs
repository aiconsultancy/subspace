//! Hints refresh what is shown even when the issue's version did not move:
//! the API keeps the version for comments, threads, attachments, links,
//! summaries, documents and reviews.
use super::{DETAIL, GROUPS, UPDATES, hub, ready_and_caught_up, sse, version, wait};
use super::{Server, server::Seen};
use crate::{
    hub::Hub,
    tests::{
        comment, common_path, detail, media::descriptor, page, pull_requests::pull_request, row,
    },
};
use serde_json::json;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

const ACTIVITY: &str = "/api/v1/issues/SUB-1/activity";
const ATTACHMENTS: &str = "/api/v1/issues/SUB-1/attachments";
const PULL_REQUESTS: &str = "/api/v1/issues/SUB-1/pull-requests";

/// What the loopback API serves; the test moves it as another person would.
#[derive(Default)]
struct Shown {
    version: AtomicI64,
    comments: AtomicI64,
    files: AtomicI64,
    /// The first attachment shown: moving it retires the ones before.
    files_from: AtomicI64,
    /// Linked pull requests.
    linked: AtomicI64,
    title: Mutex<String>,
    delay_ms: AtomicU64,
    /// My open issues, as the count read answers.
    mine: AtomicU64,
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Shown {
    fn new() -> Arc<Self> {
        let shown = Self::default();
        shown.version.store(1, Ordering::SeqCst);
        *shown.title.lock().unwrap() = "A fixture issue".into();
        Arc::new(shown)
    }

    fn title(&self) -> String {
        self.title.lock().unwrap().clone()
    }

    fn rename(&self, title: &str) {
        *self.title.lock().unwrap() = title.into();
    }

    fn detail(&self) -> String {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(self.delay_ms.load(Ordering::SeqCst)));
        let mut issue = detail("1");
        issue["version"] = self.version.load(Ordering::SeqCst).into();
        issue["title"] = self.title().into();
        issue["commentCount"] = self.comments.load(Ordering::SeqCst).into();
        self.active.fetch_sub(1, Ordering::SeqCst);
        issue.to_string()
    }

    fn groups(&self) -> String {
        let mut first = row("1");
        first["version"] = self.version.load(Ordering::SeqCst).into();
        first["title"] = self.title().into();
        json!({"groupBy":"state","groups":[{"stateId":"state-1","stateType":"started",
            "count":1,"items":[first],"nextCursor":null}]})
        .to_string()
    }

    fn pull_requests(&self) -> String {
        let rows = (0..self.linked.load(Ordering::SeqCst))
            .map(|n| pull_request(&format!("pr-{n}")))
            .collect();
        page(rows, None)
    }

    fn mine(&self) -> String {
        json!({"groupBy":"stateType","groups":[{"stateId":null,"stateType":"started",
            "count":self.mine.load(Ordering::SeqCst),"items":[],"nextCursor":null}]})
        .to_string()
    }

    fn activity(&self) -> String {
        let rows = (0..self.comments.load(Ordering::SeqCst))
            .map(|n| {
                json!({"id":format!("activity-{n}"),"kind":"comment.created",
                    "occurredAt":format!("2026-11-16T12:00:{n:02}Z"),
                    "displayAt":format!("2026-11-16T12:00:{n:02}Z"),
                    "actor":{"type":"system","userId":null,"person":null,"label":"Fixture service"},
                    "comment":comment(&format!("comment-{n}"), "Fixture comment", None)})
            })
            .collect();
        page(rows, None)
    }

    fn attachments(&self) -> String {
        let rows = (self.files_from.load(Ordering::SeqCst)..self.files.load(Ordering::SeqCst))
            .map(|n| descriptor(&format!("file-{n}")))
            .collect();
        page(rows, None)
    }

    fn answer(&self, seen: &Seen) -> (u16, String) {
        match seen.path.split('?').next().unwrap() {
            UPDATES => sse(seen),
            DETAIL => (200, self.detail()),
            GROUPS if seen.path.contains("assignee=me") => (200, self.mine()),
            GROUPS => (200, self.groups()),
            ACTIVITY => (200, self.activity()),
            ATTACHMENTS => (200, self.attachments()),
            PULL_REQUESTS => (200, self.pull_requests()),
            _ => common_path(&seen.path),
        }
    }
}

/// A live hub with the open issue and the list shown, after the stream's
/// resync has settled.
fn live(shown: &Arc<Shown>) -> (Server, Hub, super::server::Peer) {
    let answer = shown.clone();
    let server = Server::new(move |seen| answer.answer(seen));
    let hub = hub(&server);
    let mut peer = server.peer();
    ready_and_caught_up(&hub, &mut peer);
    (server, hub, peer)
}

fn comment_ids(hub: &Hub) -> Vec<String> {
    let snapshot = hub.snapshot();
    let timeline = snapshot.issue().unwrap().timeline();
    timeline
        .page()
        .map(|page| {
            page.items
                .iter()
                .filter_map(|row| row.comment.as_ref().map(|c| c.id.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_same_version_comment_hint_refreshes_the_open_issue_and_its_timeline() {
    let shown = Shown::new();
    let (server, hub, mut peer) = live(&shown);
    let details = server.count("GET", DETAIL);
    shown.comments.store(1, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "comment.created");
    let snapshot = wait(&hub, |snapshot| {
        snapshot
            .issue()
            .and_then(|issue| issue.detail())
            .is_some_and(|detail| detail.fields.comment_count == 1)
    });
    assert_eq!(
        snapshot.issue().unwrap().detail().unwrap().fields.version,
        1
    );
    assert_eq!(server.count("GET", DETAIL), details + 1);
    wait(&hub, |_| comment_ids(&hub) == ["comment-0"]);
    hub.stop();
}

#[test]
fn a_same_version_attachment_hint_reads_the_attachments_again() {
    let shown = Shown::new();
    let (server, hub, mut peer) = live(&shown);
    hub.load_attachments().unwrap();
    wait(&hub, |snapshot| {
        snapshot.issue().unwrap().files().items().is_empty()
            && !snapshot.issue().unwrap().files().is_idle()
            && !snapshot.issue().unwrap().files().is_loading()
    });
    let listed = server.count("GET", ATTACHMENTS);
    shown.files.store(1, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "attachment.added");
    // The board reads an idle attachment list again, as after an upload.
    wait(&hub, |snapshot| snapshot.issue().unwrap().files().is_idle());
    hub.load_attachments().unwrap();
    let snapshot = wait(&hub, |snapshot| {
        snapshot.issue().unwrap().files().items().len() == 1
    });
    assert_eq!(
        snapshot.issue().unwrap().files().items()[0].media_id,
        "file-0"
    );
    assert_eq!(server.count("GET", ATTACHMENTS), listed + 1);
    hub.stop();
}

#[test]
fn a_burst_during_a_read_gives_one_read_and_one_trailing_read() {
    let shown = Shown::new();
    let (server, hub, mut peer) = live(&shown);
    let details = server.count("GET", DETAIL);
    shown.delay_ms.store(1500, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "comment.created");
    wait(&hub, |_| shown.active.load(Ordering::SeqCst) == 1);
    for action in [
        "comment.created",
        "thread.resolved",
        "attachment.added",
        "pull-request.linked",
    ] {
        peer.hint("SUB-1", 1, action);
    }
    shown.rename("Renamed during the read");
    wait(&hub, |snapshot| {
        snapshot.issue().unwrap().detail().unwrap().fields.title == "Renamed during the read"
    });
    // The read in flight answers with the new title; the hints during it
    // owe one trailing read, which follows it.
    wait(&hub, |_| server.count("GET", DETAIL) == details + 2);
    std::thread::sleep(Duration::from_millis(2000));
    assert_eq!(server.count("GET", DETAIL), details + 2);
    assert_eq!(shown.peak.load(Ordering::SeqCst), 1, "one read in flight");
    hub.stop();
}

#[test]
fn an_older_reply_is_dropped() {
    let shown = Shown::new();
    let (server, hub, mut peer) = live(&shown);
    shown.version.store(5, Ordering::SeqCst);
    peer.changed("SUB-1", 5);
    wait(&hub, |_| version(&hub) == 5);
    std::thread::sleep(Duration::from_millis(700));
    let details = server.count("GET", DETAIL);
    shown.version.store(3, Ordering::SeqCst);
    shown.rename("An older reply");
    peer.changed("SUB-1", 5);
    wait(&hub, |_| server.count("GET", DETAIL) == details + 1);
    std::thread::sleep(Duration::from_millis(300));
    let snapshot = hub.snapshot();
    let fields = &snapshot.issue().unwrap().detail().unwrap().fields;
    assert_eq!(
        (fields.version, fields.title.as_str()),
        (5, "A fixture issue")
    );
    hub.stop();
}

#[test]
fn a_same_version_hint_refreshes_list_rows() {
    let shown = Shown::new();
    let (server, hub, mut peer) = live(&shown);
    let groups = server.count("GET", GROUPS);
    shown.rename("Retitled by a summary edit");
    peer.hint("SUB-1", 1, "issue.changed");
    let snapshot = wait(&hub, |snapshot| {
        snapshot.list().unwrap().groups[0].items[0].fields.title == "Retitled by a summary edit"
    });
    assert_eq!(
        snapshot.list().unwrap().groups[0].items[0].fields.version,
        1
    );
    assert!(server.count("GET", GROUPS) > groups);
    hub.stop();
}

#[test]
fn a_list_hint_reads_the_counts_again() {
    let shown = Shown::new();
    let (_server, hub, mut peer) = live(&shown);
    assert_eq!(hub.snapshot().my_issues_count(), Some(0));
    shown.mine.store(3, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "issue.assigned");
    wait(&hub, |snapshot| snapshot.my_issues_count() == Some(3));
    hub.stop();
}

#[test]
fn a_link_hint_reads_the_pull_requests_again() {
    let shown = Shown::new();
    let (_server, hub, mut peer) = live(&shown);
    let linked = |count: usize| {
        move |snapshot: &crate::hub::HubSnapshot| {
            snapshot
                .issue()
                .unwrap()
                .pull_requests()
                .is_some_and(|page| page.items.len() == count)
        }
    };
    wait(&hub, linked(0));
    shown.linked.store(1, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "pull-request.linked");
    wait(&hub, linked(1));
    shown.linked.store(0, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "pull-request.unlinked");
    wait(&hub, linked(0));
    hub.stop();
}

#[test]
fn a_removed_attachment_is_retired_after_a_complete_read() {
    let shown = Shown::new();
    shown.files.store(2, Ordering::SeqCst);
    let (_server, hub, mut peer) = live(&shown);
    hub.load_attachments().unwrap();
    wait(&hub, |snapshot| {
        snapshot.issue().unwrap().files().items().len() == 2
    });
    shown.files_from.store(1, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "attachment.removed");
    wait(&hub, |snapshot| snapshot.issue().unwrap().files().is_idle());
    // Until the list is read again, what was shown stays.
    assert_eq!(hub.snapshot().issue().unwrap().files().items().len(), 2);
    hub.load_attachments().unwrap();
    let snapshot = wait(&hub, |snapshot| {
        snapshot.issue().unwrap().files().items().len() == 1
    });
    assert_eq!(
        snapshot.issue().unwrap().files().items()[0].media_id,
        "file-1"
    );
    hub.stop();
}
