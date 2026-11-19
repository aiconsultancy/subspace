mod identifier;
mod long_values;
mod navigation;
mod paging;
mod progress;
mod workflow_end;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::offline::{Offline, offline_with};
use super::offline::{board_with_hub, offline, settle_board};
use super::{has, label, press, render};
use crate::Issues;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::dashboard::Dashboard as _;

fn opened<'a>(
    fixture: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(fixture, cx);
    board.update(cx, |board, cx| board.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |board| {
        !board.projects.is_empty()
            && board
                .issue_view
                .snapshot
                .as_ref()
                .is_some_and(|s| s.detail().is_some())
    });
    render(cx);
    (board, cx)
}

#[gpui_kit::test]
fn issue_properties_record_workflow_and_copy_keys_use_the_loaded_issue(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = opened(&fixture, cx);
    assert!(has(cx, "issue-properties"));
    assert!(has(cx, "issue-record"));
    assert_eq!(
        label(cx, "issue-workflow").as_deref(),
        Some("Workflow: In Progress, 1 of 2. In Progress → Done")
    );
    assert_eq!(
        label(cx, "issue-property:State").as_deref(),
        Some("State: In Progress")
    );
    assert_eq!(
        label(cx, "issue-property:Labels").as_deref(),
        Some("Labels: No labels")
    );
    assert!(!has(cx, "issue-property:Estimate"));
    assert!(!has(cx, "issue-property:Cycle"));
    assert!(has(cx, "issue-more"));
    for key in ["y", "ctrl-.", "cmd-."] {
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".into()))
        });
        press(cx, key);
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|i| i.text()).as_deref(),
                Some("SUB-1")
            )
        });
    }
    cx.update(|window, cx| window.click("issue-more", cx));
    press(cx, "escape");
    assert!(
        has(cx, "issues-detail"),
        "Escape closes More before leaving the issue"
    );
    cx.update(|_, cx| assert!(board.read(cx).menu.is_none()));
    for field in ["issue-summary-edit", "issue-description-edit"] {
        press(cx, "tab");
        cx.update(|window, _| assert_eq!(window.find(field).focused(), Some(true)));
    }
    press(cx, "tab");
    cx.update(|window, _| {
        assert_eq!(
            window.find("issue-child:3").focused(),
            Some(true),
            "main-column navigation precedes tracker help"
        )
    });
    cx.update(|window, cx| window.click("issue-tracker-help", cx));
    assert!(has(cx, "issue-tracker-explanation"));
    press(cx, "escape");
    assert!(!has(cx, "issue-tracker-explanation"));
    assert!(has(cx, "issues-detail"));
    cx.update(|window, _| assert_eq!(window.find("issue-tracker-help").focused(), Some(true)));
}

fn paging_fixture(switch: Arc<AtomicBool>) -> Offline {
    offline_with(move |request| {
        let path = &request.path;
        if path.contains("/workflow-states?") {
            let mut page = super::offline::api(path, &Default::default());
            page["items"].as_array_mut().unwrap().extend([
                json!({"id":"triage","projectId":"project-SUB","name":"Triage","type":"triage","position":-2,"isDefault":false}),
                json!({"id":"review","projectId":"project-SUB","name":"In review","type":"started","position":3,"isDefault":false}),
                json!({"id":"cancelled","projectId":"project-SUB","name":"Cancelled","type":"canceled","position":4,"isDefault":false}),
                json!({"id":"duplicate","projectId":"project-SUB","name":"Duplicate","type":"duplicate","position":5,"isDefault":false}),
                json!({"id":"todo","projectId":"project-SUB","name":"Todo","type":"unstarted","position":0,"isDefault":false}),
                json!({"id":"backlog","projectId":"project-SUB","name":"Backlog","type":"backlog","position":-1,"isDefault":false}),
            ]);
            return Some((200, page.to_string()));
        }
        if path.contains("/pull-requests?") && !path.contains("cursor=") {
            return Some((200, json!({"items":[],"nextCursor":"prs-next"}).to_string()));
        }
        if path.contains("/relations?") || path.contains("/pull-requests?") {
            return Some(if switch.load(Ordering::Relaxed) {
                (500, "{}".into())
            } else {
                (200, json!({"items": [], "nextCursor": null}).to_string())
            });
        }
        if path.ends_with("/issues/SUB-1") {
            let mut detail = super::offline::api("/api/v1/issues/SUB-1", &Default::default());
            detail["identifier"] = json!("SUB-1");
            detail["relations"] = json!({"items": [], "nextCursor": "relations-next"});
            detail["blocked"] = json!(true);
            return Some((200, detail.to_string()));
        }
        None
    })
}

#[gpui_kit::test]
fn narrow_issue_keeps_the_current_beacon_and_stacks_aligned_panels(cx: &mut TestAppContext) {
    let fixture = paging_fixture(Arc::default());
    let (hub, address) = super::offline::fixture_hub(&fixture);
    // Six stops (five path steps and the end slot) need more than this.
    let (board, cx) = super::mount_at(cx, 560.);
    cx.update(|window, cx| {
        crate::link::install(cx, hub, Some(address));
        board.update(cx, |board, cx| {
            board.activate(window, cx);
            board.open_detail("SUB-1".into(), cx);
        });
    });
    settle_board(&board, cx, |board| {
        !board.projects.is_empty()
            && board
                .issue_view
                .snapshot
                .as_ref()
                .is_some_and(|s| s.detail().is_some())
    });
    render(cx);
    render(cx);
    assert!(has(cx, "issue-workflow-current"));
    cx.update(|window, _| {
        let main = window.find("issue-main-column").bounds();
        let properties = window.find("issue-properties").bounds();
        let record = window.find("issue-record").bounds();
        assert_eq!(main.left(), properties.left());
        assert_eq!(main.size.width, properties.size.width);
        assert!(properties.top() >= main.bottom());
        assert_eq!(record.left(), properties.left());
        assert!(properties.right() <= gpui_kit::px(560.));
    });
}

#[gpui_kit::test]
fn relations_and_pull_requests_page_retry_and_keep_the_issue(cx: &mut TestAppContext) {
    let fail = Arc::new(AtomicBool::new(true));
    let fixture = paging_fixture(fail.clone());
    let (board, cx) = opened(&fixture, cx);
    assert!(has(cx, "issue-relations"));
    assert!(has(cx, "issue-pull-requests"));
    assert!(has(cx, "issue-blocked"));
    for (button, rows) in [
        ("issue-more-relations", true),
        ("issue-more-pull-requests", false),
    ] {
        cx.update(|window, cx| window.click(button, cx));
        settle_board(&board, cx, |board| {
            let s = board.issue_view.snapshot.as_ref().unwrap();
            if rows {
                s.relations_failed()
            } else {
                s.pull_requests_failed()
            }
        });
        render(cx);
        assert!(label(cx, button).is_some_and(|l| l.starts_with("Retry")));
        paging::assert_first_page_failure(cx, rows);
        assert!(has(cx, "issue-title"));
    }
    fail.store(false, Ordering::Relaxed);
    for button in ["issue-more-relations", "issue-more-pull-requests"] {
        cx.update(|window, cx| window.click(button, cx));
    }
    settle_board(&board, cx, |board| {
        let s = board.issue_view.snapshot.as_ref().unwrap();
        s.detail()
            .unwrap()
            .relations
            .as_ref()
            .unwrap()
            .next_cursor
            .is_none()
            && s.pull_requests().is_some_and(|p| p.next_cursor.is_none())
    });
    render(cx);
    assert!(!has(cx, "issue-more-relations"));
    assert!(!has(cx, "issue-more-pull-requests"));
    cx.update(|window, cx| {
        assert!(
            board.read(cx).focus.is_focused(window),
            "the exhausted page returns focus to issue navigation"
        )
    });
}

#[gpui_kit::test]
fn an_undeployed_pull_request_route_is_calm_and_keeps_other_panels(cx: &mut TestAppContext) {
    let fixture = offline_with(|request| {
        request
            .path
            .contains("/pull-requests?")
            .then(|| (404, "{}".into()))
    });
    let (board, cx) = opened(&fixture, cx);
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .is_some_and(|i| i.pull_requests_unavailable())
    });
    render(cx);
    assert!(has(cx, "issue-pull-requests-unavailable"));
    assert_eq!(
        label(cx, "issue-pull-requests-unavailable").as_deref(),
        Some("Pull requests are not available from this server yet")
    );
    assert!(!has(cx, "issue-more-pull-requests"));
    assert!(has(cx, "issue-title"));
    assert!(has(cx, "issue-properties"));
    assert!(has(cx, "issue-record"));
    cx.update(|_, cx| {
        let board = board.read(cx);
        let issue = board.issue_view.snapshot.as_ref().unwrap();
        assert!(!issue.pull_requests_failed());
        assert!(
            issue
                .detail()
                .unwrap()
                .fields
                .pull_request_summary
                .is_none()
        );
        assert!(
            !board
                .palette_commands()
                .iter()
                .any(|c| c.label.contains("pull requests"))
        );
    });
}
