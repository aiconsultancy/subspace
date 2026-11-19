//! The issue page's end slot: its menu, its write and its read-only cases.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Subscription, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_api_client::hub::IssueStatus;
use subspace_framework::dashboard::DashboardEvent;

use crate::Issues;
use crate::tests::offline::{DONE, Offline, board_with_hub, offline_with, row, settle_board};
use crate::tests::{has, label, press, render};

const STARTED: (&str, &str, &str) = ("state-started", "In Progress", "started");
const CANCELLED: (&str, &str, &str) = ("state-cancelled", "Cancelled", "canceled");

fn issue(state: (&str, &str, &str), version: u32) -> String {
    let mut value = row("2", "Fixture plain issue");
    value["todos"] = json!([]);
    value["relations"] = json!({"items": [], "nextCursor": null});
    value["state"] = json!({"id": state.0, "name": state.1, "type": state.2});
    value["version"] = json!(version);
    value.to_string()
}

fn workflow_states() -> String {
    let state = |id: &str, name: &str, kind: &str, position: i32| {
        json!({"id": id, "projectId": "project-SUB", "name": name, "type": kind,
            "position": position, "isDefault": position == 1})
    };
    json!({"items": [
        state(STARTED.0, STARTED.1, STARTED.2, 1),
        state(CANCELLED.0, CANCELLED.1, CANCELLED.2, 3),
        state(DONE, "Done", "completed", 2),
        state("state-duplicate", "Duplicate", "duplicate", 4),
    ], "nextCursor": null})
    .to_string()
}

/// SUB-2 starts In Progress; a PATCH moves it to Cancelled, or `refuse`
/// answers that it changed elsewhere.
fn fixture(refuse: bool) -> Offline {
    let patched = Arc::new(AtomicBool::new(false));
    offline_with(move |r| {
        if r.path.contains("/workflow-states") {
            return Some((200, workflow_states()));
        }
        if r.method == "PATCH" && r.path == "/api/v1/issues/2" {
            if refuse {
                return Some((409, json!({"currentVersion": 2}).to_string()));
            }
            patched.store(true, Ordering::SeqCst);
            return Some((200, issue(CANCELLED, 2)));
        }
        if r.path == "/api/v1/issues/2" || r.path == "/api/v1/issues/SUB-2" {
            let cancelled = patched.load(Ordering::SeqCst);
            return Some((
                200,
                issue(
                    if cancelled { CANCELLED } else { STARTED },
                    1 + u32::from(cancelled),
                ),
            ));
        }
        None
    })
}

fn opened<'a>(
    fixture: &Offline,
    identifier: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    board.update(cx, |b, cx| b.open_detail(identifier.into(), cx));
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| s.detail().is_some())
    });
    render(cx);
    render(cx);
    (board, cx)
}

/// Clicks the end slot and waits for its menu.
fn open_end_menu(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    cx.update(|w, cx| w.click("issue-end-state", cx));
    settle_board(board, cx, |b| b.menu.is_some());
}

fn patches(fixture: &Offline) -> Vec<Value> {
    fixture
        .mock
        .requests()
        .into_iter()
        .filter(|r| r.method == "PATCH")
        .map(|r| serde_json::from_str(&r.body).unwrap())
        .collect()
}

fn options(cx: &mut VisualTestContext) -> usize {
    cx.update(|w, _| {
        (0..9_usize)
            .take_while(|ix| w.try_find(("menu-option", *ix)).is_some())
            .count()
    })
}

fn toasts(
    board: &Entity<Issues>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Vec<String>>>, Subscription) {
    let toasts = Rc::new(RefCell::new(Vec::new()));
    let received = toasts.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(board, move |_, event, _| {
            if let DashboardEvent::Toast(toast) = event {
                received.borrow_mut().push(toast.text.to_string());
            }
        })
    });
    (toasts, subscription)
}

#[gpui_kit::test]
fn the_end_slot_menu_moves_an_issue_to_cancelled(cx: &mut TestAppContext) {
    let fixture = fixture(false);
    let (board, cx) = opened(&fixture, "SUB-2", cx);
    assert_eq!(
        label(cx, "issue-workflow").as_deref(),
        Some(
            "Workflow: In Progress, 1 of 2. In Progress → Done. End states: Done, Cancelled, Duplicate"
        )
    );
    assert_eq!(
        label(cx, "issue-end-state").as_deref(),
        Some("End state: Done. Change end state")
    );
    open_end_menu(&board, cx);
    render(cx);
    assert_eq!(options(cx), 3, "Done, Cancelled and Duplicate");
    cx.update(|w, cx| w.click(("menu-option", 1_usize), cx));
    // The slot stays busy until a reload carrying the written version has
    // returned, so when it is free the page already shows the new state. (A
    // later refresh may already be under way; a reload keeps the content.)
    settle_board(&board, cx, |b| !b.changing_state("2"));
    cx.update(|_, cx| {
        let board = board.read(cx);
        let open = board.issue_view.snapshot.as_ref().unwrap();
        let detail = open.detail().unwrap();
        assert_eq!(detail.fields.state.id, CANCELLED.0);
        assert_eq!(detail.fields.version, 2);
    });
    // Let any later refresh finish before reading the slot's labels.
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| *s.status() == IssueStatus::Loaded)
    });
    render(cx);
    assert_eq!(
        patches(&fixture),
        [json!({"version": 1, "stateId": CANCELLED.0})]
    );
    cx.update(|_, cx| assert!(board.read(cx).menu.is_none()));
    assert_eq!(
        label(cx, "issue-end-state").as_deref(),
        Some("End state: Cancelled. Change end state")
    );
    assert!(label(cx, "issue-workflow").is_some_and(|text| {
        text.starts_with("Workflow: Cancelled, 2 of 2. In Progress → Cancelled.")
    }));
}

#[gpui_kit::test]
fn the_end_slot_opens_from_the_keyboard_and_returns_focus(cx: &mut TestAppContext) {
    let fixture = fixture(false);
    let (board, cx) = opened(&fixture, "SUB-2", cx);
    open_end_menu(&board, cx);
    press(cx, "escape");
    cx.update(|w, cx| {
        assert!(board.read(cx).menu.is_none());
        assert_eq!(w.find("issue-end-state").focused(), Some(true));
    });
    for key in ["enter", "space"] {
        press(cx, key);
        cx.update(|_, cx| assert!(board.read(cx).menu.is_some(), "{key} opens the menu"));
        assert_eq!(options(cx), 3);
        press(cx, "escape");
    }
    press(cx, "enter");
    press(cx, "3");
    settle_board(&board, cx, |b| {
        !b.changing_state("2") && !patches(&fixture).is_empty()
    });
    assert_eq!(
        patches(&fixture),
        [json!({"version": 1, "stateId": "state-duplicate"})],
        "the board has no duplicate-of picker, so Duplicate is a state change"
    );
}

#[gpui_kit::test]
fn a_refused_change_reports_the_issue_and_shows_its_latest_state(cx: &mut TestAppContext) {
    let fixture = fixture(true);
    let (board, cx) = opened(&fixture, "SUB-2", cx);
    let (toasts, _subscription) = toasts(&board, cx);
    open_end_menu(&board, cx);
    render(cx);
    cx.update(|w, cx| w.click(("menu-option", 1_usize), cx));
    settle_board(&board, cx, |b| {
        !b.changing_state("2") && !toasts.borrow().is_empty() && b.can_edit_issue()
    });
    render(cx);
    assert_eq!(
        *toasts.borrow(),
        ["SUB-2 changed since it was read. Its latest state is shown."]
    );
    assert_eq!(
        label(cx, "issue-end-state").as_deref(),
        Some("End state: Done. Change end state")
    );
}

#[gpui_kit::test]
fn a_tracker_shows_its_end_state_without_a_control(cx: &mut TestAppContext) {
    let fixture = fixture(false);
    let (_board, cx) = opened(&fixture, "SUB-1", cx);
    assert!(has(cx, "issue-workflow"));
    assert!(
        !has(cx, "issue-end-state"),
        "a tracker's state comes from its sub-issues"
    );
}

#[gpui_kit::test]
fn leaving_the_issue_mid_write_leaves_the_next_issue_alone(cx: &mut TestAppContext) {
    let fixture = fixture(false);
    let (board, cx) = opened(&fixture, "SUB-2", cx);
    open_end_menu(&board, cx);
    render(cx);
    // Pick Cancelled, then open SUB-1 before the write has run: the pick
    // starts the write when its update ends, and the write only runs once
    // the test lets tasks run.
    cx.update(|w, cx| w.click(("menu-option", 1_usize), cx));
    cx.update(|_, cx| {
        assert!(
            board.read(cx).changing_state("2"),
            "the write is on its way"
        );
        board.update(cx, |b, cx| b.open_detail("SUB-1".into(), cx));
    });
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| s.identifier() == "SUB-1" && *s.status() == IssueStatus::Loaded)
    });
    settle_board(&board, cx, |_| patches(&fixture).len() == 1);
    cx.run_until_parked();
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| *s.status() == IssueStatus::Loaded)
    });
    assert_eq!(
        patches(&fixture),
        [json!({"version": 1, "stateId": CANCELLED.0})]
    );
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(board.issue_view.state_change.is_none());
        assert!(!board.changing_state("2"));
        let open = board.issue_view.snapshot.as_ref().unwrap();
        assert_eq!(open.identifier(), "SUB-1");
        assert_eq!(
            open.detail().unwrap().fields.state.id,
            STARTED.0,
            "SUB-2's result is not applied to SUB-1"
        );
    });
}

#[gpui_kit::test]
fn reopening_the_issue_mid_write_shows_the_written_state(cx: &mut TestAppContext) {
    let fixture = fixture(false);
    let (board, cx) = opened(&fixture, "SUB-2", cx);
    open_end_menu(&board, cx);
    render(cx);
    // Pick Cancelled, then leave SUB-2 and come back before the write has
    // run. Reopening forgets the change; the reopened page still ends on the
    // written state, and SUB-2 is read again after the write. (This harness
    // always serves the reopened read after the write, so it cannot stage the
    // read arriving first.)
    cx.update(|w, cx| w.click(("menu-option", 1_usize), cx));
    board.update(cx, |b, cx| {
        b.open_detail("SUB-1".into(), cx);
        b.open_detail("SUB-2".into(), cx);
    });
    settle_board(&board, cx, |_| patches(&fixture).len() == 1);
    cx.run_until_parked();
    settle_board(&board, cx, |b| {
        b.issue_view.snapshot.as_ref().is_some_and(|s| {
            s.identifier() == "SUB-2"
                && s.detail().is_some_and(|d| d.fields.state.id == CANCELLED.0)
        })
    });
    let requests = fixture.mock.requests();
    let patch = requests.iter().position(|r| r.method == "PATCH").unwrap();
    assert!(
        requests[patch..].iter().any(|r| r.method == "GET"
            && (r.path == "/api/v1/issues/2" || r.path == "/api/v1/issues/SUB-2")),
        "SUB-2 is read again after the write"
    );
}
