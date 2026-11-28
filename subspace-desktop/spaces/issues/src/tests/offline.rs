//! Synthetic numeric-loopback Subspace API for board interaction tests.
use crate::{Issues, fixture, link};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU16, Ordering},
};
use std::time::{Duration, Instant};
use subspace_api_client::hub::Hub;
use subspace_framework::client_test_support::{Mock, Request};
use subspace_framework::dashboard::Dashboard as _;
pub(crate) const TITLE: &str = "Fixture sample tracker";
pub(crate) const LATER_TITLE: &str = "Fixture later sample";
pub(crate) const CURSOR: &str = "fixture-cursor-secret";
pub(crate) const DONE: &str = "state-done";
#[derive(Default)]
pub(crate) struct Switches {
    pub(crate) fail_lists: AtomicBool,
    pub(crate) fail_counts: AtomicBool,
    pub(crate) offline_lists: AtomicBool,
    pub(crate) hold_pages: AtomicBool,
    pub(crate) empty_pages: AtomicBool,
    pub(crate) nested_done_child: AtomicBool,
    pub(crate) refuse_cursors: AtomicBool,
    pub(crate) offline_issue: AtomicBool,
    pub(crate) issue_error: AtomicU16,
    pub(crate) hold_issue: AtomicBool,
    pub(crate) fail_children: AtomicBool,
    pub(crate) hold_lists: AtomicBool,
}
pub(crate) struct Offline {
    pub(crate) mock: Mock,
    pub(crate) switches: Arc<Switches>,
}
pub(crate) fn row(id: &str, title: &str) -> Value {
    json!({"id":id,"identifier":format!("SUB-{id}"),"projectId":"project-SUB","projectKey":"SUB",
        "type":"issue","title":title,"summary":if id == "1" { None } else { Some("Keep fixture issues readable so people can understand the work before opening its description.") },"state":{"id":"state-started","name":"In Progress","type":"started"},
        "priority":2,"labels":[],"blocked":false,"createdAt":"2026-11-01T00:00:00Z",
        "updatedAt":"2026-11-01T00:00:00Z","lastActivityAt":"2026-11-01T00:00:00Z",
        "version":1,"commentCount":0,"todoCounts":{"total":0,"done":0}})
}
fn hold(held: &AtomicBool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while held.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn refusal(path: &str, switches: &Switches) -> Option<(u16, String)> {
    list_refusal(path, switches)
        .or_else(|| scalar_refusal(path, switches))
        .or_else(|| page_refusal(path, switches))
}
fn list_refusal(path: &str, switches: &Switches) -> Option<(u16, String)> {
    if !path.contains("issues/groups") {
        return None;
    }
    hold(&switches.hold_lists);
    if switches.offline_lists.load(Ordering::Relaxed) {
        return Some((0, String::new()));
    }
    let failed = switches.fail_lists.load(Ordering::Relaxed)
        || (path.contains("limitPerGroup=0") && switches.fail_counts.load(Ordering::Relaxed));
    failed.then(|| (500, "{}".into()))
}
fn scalar_refusal(path: &str, switches: &Switches) -> Option<(u16, String)> {
    if !path.contains("/issues/SUB-") || path.contains("/comments") || path.contains("/activity") {
        return None;
    }
    hold(&switches.hold_issue);
    if switches.offline_issue.load(Ordering::Relaxed) {
        return Some((0, String::new()));
    }
    let status = switches.issue_error.load(Ordering::Relaxed);
    (status != 0).then(|| (status, "{}".into()))
}
fn page_refusal(path: &str, switches: &Switches) -> Option<(u16, String)> {
    if path.contains("parent=") && switches.fail_children.load(Ordering::Relaxed) {
        return Some((500, "{}".into()));
    }
    if path.contains(CURSOR) && switches.refuse_cursors.load(Ordering::Relaxed) {
        return Some((400, "{}".into()));
    }
    None
}
pub(crate) fn api(path: &str, switches: &Switches) -> Value {
    if path.contains("limitPerGroup=0") {
        let count = if path.contains("assignee=me") { 3 } else { 15 };
        return json!({"groupBy":"stateType","groups":[{"stateType":"started","count":count,"items":[],"nextCursor":null}]});
    }
    if let Some(value) = metadata(path) {
        return value;
    }
    if path.contains("issues/groups") {
        return json!({"groupBy":"state","groups":[
        {"stateId":"state-started","stateType":"started","count":2,"items":[row("1",TITLE)],"nextCursor":CURSOR},
        {"stateId":DONE,"stateType":"completed","count":1,"items":[],"nextCursor":null}]});
    }
    if [
        "/comments",
        "/activity",
        "/relations",
        "/images",
        "/attachments",
        "/pull-requests",
    ]
    .iter()
    .any(|part| path.contains(part))
    {
        return json!({"items":[],"nextCursor":null});
    }
    if let Some((_, identifier)) = path.split_once("/issues/SUB-") {
        return detail(identifier);
    }
    group_page(path, switches)
}
fn group_page(path: &str, switches: &Switches) -> Value {
    if path.contains(DONE) {
        hold(&switches.hold_pages);
        if switches.empty_pages.load(Ordering::Relaxed) {
            return json!({"items":[],"nextCursor":null});
        }
        if switches.nested_done_child.load(Ordering::Relaxed) {
            let mut child = row("3", "Finished child");
            child["parentId"] = json!("1");
            child["state"] = json!({"id":DONE,"name":"Done","type":"completed"});
            return json!({"items":[child],"nextCursor":null});
        }
    }
    json!({"items":[row("2",LATER_TITLE)],"nextCursor":null})
}
fn metadata(path: &str) -> Option<Value> {
    if path.ends_with("/me") {
        return Some(json!({"userId":"fixture-me","kind":"human","displayName":"Ada Fixture"}));
    }
    if path.contains("/workflow-states") {
        return Some(
            json!({"items":[{"id":"state-started","projectId":"project-SUB","name":"In Progress","type":"started","position":1,"isDefault":true},{"id":DONE,"projectId":"project-SUB","name":"Done","type":"completed","position":2,"isDefault":false}],"nextCursor":null}),
        );
    }
    if path.contains("/projects") {
        return Some(
            json!({"items":[{"id":"project-SUB","key":"SUB","name":"Subspace","defaultStateId":"state-started"}],"nextCursor":null}),
        );
    }
    None
}
fn detail(identifier: &str) -> Value {
    let mut issue = row(
        identifier,
        if identifier == "1" {
            TITLE
        } else {
            LATER_TITLE
        },
    );
    issue["description"] = json!("Price $5. [Tracker](/issues/SUB-1) and text.");
    issue["todos"] = json!([]);
    issue["relations"] = json!({"items":[],"nextCursor":null});
    if identifier == "1" {
        issue["type"] = json!("tracker");
        issue["children"] =
            json!({"items":[row("3","Fixture child")],"nextCursor":"children-second"});
        issue["progress"] =
            json!({"done":0,"total":2,"canceled":0,"duplicate":0,"sample":[],"hasMore":true});
    }
    issue
}
pub(crate) fn offline() -> Offline {
    offline_with(|_| None)
}
pub(crate) fn offline_with(
    custom: impl Fn(&Request) -> Option<(u16, String)> + Send + Sync + 'static,
) -> Offline {
    let switches = Arc::new(Switches::default());
    let answers = switches.clone();
    let mock = Mock::with_bytes(move |request| {
        let (status, body) = custom(request)
            .or_else(|| refusal(&request.path, &answers))
            .unwrap_or_else(|| (200, api(&request.path, &answers).to_string()));
        let headers = if status == 403 && request.path.starts_with("/api/v1/decisions") {
            vec![("content-type".into(), "application/problem+json".into())]
        } else {
            Vec::new()
        };
        (status, headers, body.into_bytes())
    });
    Offline { mock, switches }
}
pub(crate) fn fixture_hub(offline: &Offline) -> (Hub, String) {
    fixture::hub_for(Some(&format!("{}/api", offline.mock.base_url())), None).unwrap()
}
pub(crate) fn board_with_hub<'a>(
    offline: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    let (board, cx) = super::mount(cx);
    attach_hub(offline, &board, cx);
    (board, cx)
}

pub(crate) fn attach_hub(offline: &Offline, board: &Entity<Issues>, cx: &mut VisualTestContext) {
    cx.executor().allow_parking();
    let (hub, address) = fixture_hub(offline);
    cx.update(|window, cx| {
        link::install(cx, hub, Some(address));
        board.update(cx, |board, cx| {
            board.set_project(crate::model::ProjectScope::Project("SUB".into()), cx);
            board.activate(window, cx);
        });
    });
}
pub(crate) fn settle_board(
    board: &Entity<Issues>,
    cx: &mut VisualTestContext,
    done: impl Fn(&Issues) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        board.update(cx, |board, cx| board.poll_hub(cx));
        if cx.update(|_, cx| done(board.read(cx))) {
            return;
        }
        assert!(Instant::now() < deadline, "the offline hub didn't settle");
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn wait_for(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "the fixture did not settle");
        std::thread::sleep(Duration::from_millis(10));
    }
}
