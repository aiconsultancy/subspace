//! Tracker commands and the message toolbar on the real conversation:
//! completion offers the commands only with a tracker, a refused command
//! keeps its draft, a landed one says what changed and asks for the issue
//! again, and the toolbar shows on the hovered message and opens the Make
//! issue dialog, which alone writes. No message names an issue: without
//! an answering Space, asking about one would drop the scope. Fictional
//! names and issues only.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use serde_json::json;
use subspace_client_irc::{Command, Event, Line};
use subspace_framework::client_test_support::Mock;
use subspace_framework::contracts::issues::IssueScope;

use crate::conversation::Conversation;
use crate::fixtures::{channel, message};
use crate::hub::Hub;
use crate::tests::completion::{focused_composer as composer, labels, reveal, text};
use crate::tests::{channel_events, inject, mount, settings};

mod composer;
mod destination;
mod dialog;
mod unconfirmed;

fn with_id(mut line: Line, id: &str) -> Line {
    line.id = Some(id.into());
    line
}

/// `#subspace` with alice's `m1` and bob's `m2`; the hub has
/// a tracker scope for `NAV` when `tracker`, and writes to `mock`.
fn open<'a>(
    cx: &'a mut TestAppContext,
    tracker: bool,
    mock: Option<&Mock>,
) -> (Entity<Conversation>, Entity<Hub>, &'a mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    cx.update(|_, cx| {
        subspace_framework::dashboard::chat_input::bind_keys(cx);
        crate::conversation::bind_keys(cx);
    });
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(Event::Line(with_id(
        message(&room, "alice", "We need a decision on decode", 3),
        "m1",
    )));
    events.push(Event::Line(with_id(
        message(&room, "bob", "Pan stalls while tiles decode", 4),
        "m2",
    )));
    inject(&hub, events, cx);
    let origin = mock.map_or_else(
        || "http://127.0.0.1:9".to_owned(),
        |mock| format!("{}/api", mock.base_url()),
    );
    hub.update(cx, |hub, cx| {
        if tracker {
            let scope =
                IssueScope::new("subspace-api", "Subspace API", &origin).with_project_keys(["NAV"]);
            hub.issues.set_scope(Some(scope));
        }
        if let Some(mock) = mock {
            let url = format!("{}/api", mock.base_url());
            let client = subspace_api_client::Client::loopback(&url, None)
                .unwrap()
                .without_live();
            hub.tracker.test_connect(client);
        }
        cx.notify();
    });
    (reveal(&board, &room, cx), hub, cx)
}

use crate::tests::completion::submit;

/// What the hub said to IRC: lines and replies, not the board's own
/// housekeeping or typing notices.
fn said(hub: &Hub) -> Vec<&Command> {
    hub.sent()
        .iter()
        .filter(|command| {
            matches!(
                command,
                Command::Input { .. } | Command::Say { .. } | Command::Reply { .. }
            )
        })
        .collect()
}

fn seq(hub: &Entity<Hub>, id: &str, cx: &mut VisualTestContext) -> i64 {
    hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&channel("#subspace"))
            .and_then(|held| {
                held.entries
                    .iter()
                    .find(|entry| entry.line.id.as_deref() == Some(id))
            })
            .map(|entry| entry.seq)
            .expect("held")
    })
}

fn notice(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        window
            .try_find("irc-reaction-notice")
            .and_then(|found| found.label().map(str::to_owned))
            .unwrap_or_default()
    })
}

/// A tracker where the person is signed in and no issue exists.
fn signed_in() -> Mock {
    Mock::new(|request| match request.path.as_str() {
        "/api/v1/me" => (200, me()),
        _ => (404, "{}".into()),
    })
}

fn me() -> String {
    json!({"userId":"user-me","kind":"human","displayName":"Mira"}).to_string()
}

#[gpui_kit::test]
fn the_commands_are_offered_only_with_a_tracker(cx: &mut TestAppContext) {
    let (conversation, _hub, cx) = open(cx, true, None);
    let _input = composer(&conversation, cx);
    cx.simulate_input("/");
    let offered = labels(&conversation, cx);
    assert!(
        ["/assign", "/ready", "/decision"]
            .iter()
            .all(|command| offered.iter().any(|label| label == command)),
        "{offered:?}"
    );
    cx.simulate_input("as");
    assert_eq!(
        labels(&conversation, cx).first().map(String::as_str),
        Some("/assign")
    );
}

#[gpui_kit::test]
fn without_a_tracker_a_command_is_kept_and_never_sent_to_irc(cx: &mut TestAppContext) {
    let (conversation, hub, cx) = open(cx, false, None);
    let input = composer(&conversation, cx);
    cx.simulate_input("/as");
    assert!(
        labels(&conversation, cx)
            .iter()
            .all(|label| label != "/assign"),
        "not offered"
    );
    submit(&input, "/assign NAV-2 to me", cx);
    assert_eq!(text(&input, cx), "/assign NAV-2 to me", "the draft stays");
    assert_eq!(
        input.read_with(cx, |input, _| input.error().map(str::to_owned)),
        Some(crate::tracker::NO_TRACKER.to_owned())
    );
    hub.read_with(cx, |hub, _| {
        assert!(said(hub).is_empty(), "nothing went to IRC: {:?}", said(hub))
    });
}

#[gpui_kit::test]
fn a_refused_write_keeps_the_draft_and_says_why(cx: &mut TestAppContext) {
    let mock = signed_in();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let input = composer(&conversation, cx);
    cx.simulate_input("/ready NAV-9");
    assert_eq!(
        labels(&conversation, cx).first().map(String::as_str),
        Some("/ready"),
        "the first row runs the command"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        notice(cx),
        "NAV-9 isn't in the tracker, or you can't see it."
    );
    assert_eq!(
        text(&input, cx),
        "/ready NAV-9",
        "a failed write keeps the draft"
    );
    conversation.read_with(cx, |conversation, _| {
        assert!(conversation.tracker.refreshed.is_empty())
    });
    hub.read_with(cx, |hub, _| {
        assert!(said(hub).is_empty(), "{:?}", said(hub));
    });
    assert!(
        mock.requests()
            .iter()
            .all(|request| request.method == "GET")
    );
}

/// The tracker the Make issue test writes to: `me`, the NAV project and
/// one created issue.
fn creating() -> Mock {
    Mock::new(
        |request| match (request.method.as_str(), request.path.as_str()) {
            (_, "/api/v1/me") => (200, me()),
            ("GET", path) if path.starts_with("/api/v1/projects") => (200, projects()),
            ("POST", "/api/v1/issues") => (201, created().to_string()),
            _ => (404, "{}".into()),
        },
    )
}

fn projects() -> String {
    json!({"items":[{"id":"project-nav","key":"NAV","name":"Navigation","defaultStateId":"state-1",
        "initiativeId":null,"archivedAt":null}],"nextCursor":null})
    .to_string()
}

fn found(id: String, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| window.try_find(SharedString::from(id)).is_some())
}

#[gpui_kit::test]
fn the_toolbar_shows_on_the_hovered_message_and_hovering_writes_nothing(cx: &mut TestAppContext) {
    let mock = creating();
    let (conversation, hub, cx) = open(cx, true, Some(&mock));
    let m2 = seq(&hub, "m2", cx);
    assert!(!found(format!("irc-line:{m2}:toolbar"), cx));
    conversation.update(cx, |conversation, cx| {
        crate::conversation::hover(conversation, m2, true, cx)
    });
    cx.update(|window, cx| window.render_frame(cx));
    assert!(found(format!("irc-line:{m2}:toolbar"), cx));
    assert!(found(format!("irc-line:{m2}:make-issue"), cx));
    assert!(
        !found(format!("irc-line:{m2}:reply"), cx),
        "no Thread tab in #subspace, so no reply"
    );
    assert!(
        !found(format!("irc-line:{m2}:record-decision"), cx),
        "no issue to record a decision on"
    );
    conversation.update(cx, |conversation, cx| {
        crate::conversation::hover(conversation, m2, false, cx)
    });
    cx.update(|window, cx| window.render_frame(cx));
    assert!(!found(format!("irc-line:{m2}:toolbar"), cx));
    assert!(mock.requests().is_empty(), "hovering writes nothing");
}

/// Opens Make issue on bob's `m2` from its toolbar.
fn open_make_issue(
    conversation: &Entity<Conversation>,
    hub: &Entity<Hub>,
    cx: &mut VisualTestContext,
) {
    open_make_issue_on(conversation, (hub, "m2"), cx);
}

/// Opens Make issue on message `id` from its toolbar.
fn open_make_issue_on(
    conversation: &Entity<Conversation>,
    (hub, id): (&Entity<Hub>, &str),
    cx: &mut VisualTestContext,
) {
    let m2 = seq(hub, id, cx);
    conversation.update(cx, |conversation, cx| {
        crate::conversation::hover(conversation, m2, true, cx)
    });
    cx.update(|window, cx| window.render_frame(cx));
    let make = SharedString::from(format!("irc-line:{m2}:make-issue"));
    cx.update(|window, cx| window.click(make, cx));
    cx.update(|window, cx| window.render_frame(cx));
    assert!(found("irc-tracker-dialog".into(), cx));
}

fn created() -> serde_json::Value {
    serde_json::from_str(r#"{"id":"issue-14","identifier":"NAV-14","projectId":"project-nav","projectKey":"NAV",
        "type":"issue","title":"Pan stalls while tiles decode","summary":"Pan stalls while tiles decode",
        "state":{"id":"state-1","name":"Todo","type":"unstarted"},"assignee":null,"labels":[],
        "priority":0,"estimate":null,"dueDate":null,"milestoneId":null,"parentId":null,"blocked":false,
        "archivedAt":null,"createdAt":"2026-11-22T12:00:00Z","updatedAt":"2026-11-22T12:00:00Z",
        "version":1,"progress":null,"commentCount":0,"todoCounts":{"total":0,"done":0},
        "lastActivityAt":"2026-11-22T12:00:00Z","lastActivity":null,"cycle":null,"relayedCrew":null,
        "provenance":null,"description":"","todos":[],"createdBy":null,"startedAt":null,
        "completedAt":null,"canceledAt":null,"archivedBy":null,"parent":null,"duplicateOf":null,
        "children":{"items":[],"nextCursor":null},"relations":{"items":[],"nextCursor":null},"mentions":[]}"#).unwrap()
}
