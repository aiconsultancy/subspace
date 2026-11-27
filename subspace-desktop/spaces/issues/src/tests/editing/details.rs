use super::{board_with_hub, has, issue, offline_with, patch_bodies, render, settle_board};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

fn opened(
    cx: &mut TestAppContext,
) -> (
    super::Offline,
    Entity<crate::Issues>,
    &mut VisualTestContext,
) {
    let fixture = offline_with(
        |r| match (r.method.as_str(), r.path.split('?').next().unwrap()) {
            ("PATCH", "/api/v1/issues/2") => Some((200, issue("Original", 2))),
            ("GET", "/api/v1/issues/SUB-2" | "/api/v1/issues/2") => {
                Some((200, issue("Original", 1)))
            }
            ("GET", "/api/v1/issues/SUB-3") => {
                let mut archived: serde_json::Value =
                    serde_json::from_str(&issue("Archived", 1)).unwrap();
                archived["archivedAt"] = json!("2026-11-01T00:00:00Z");
                Some((200, archived.to_string()))
            }
            ("GET", "/api/v1/projects/SUB/milestones") => Some((
                200,
                json!({"items":[{"id":"milestone-1","name":"First delivery"}],"nextCursor":null})
                    .to_string(),
            )),
            ("GET", "/api/v1/issue-cycles") => Some((
                200,
                json!({"items":[{"id":"cycle-1","number":1,"state":"current"}],"nextCursor":null})
                    .to_string(),
            )),
            _ => None,
        },
    );
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    super::open_page(&board, cx);
    (fixture, board, cx)
}

#[gpui_kit::test]
fn properties_open_the_matching_picker_and_send_a_sparse_versioned_change(cx: &mut TestAppContext) {
    let (fixture, board, cx) = opened(cx);
    assert!(has(cx, "issue-property-edit:Project"));
    cx.update(|w, cx| w.click("issue-property-edit:Priority", cx));
    super::properties::wait_for_option(cx, 0);
    render(cx);
    assert!(has(cx, "issues-detail"));
    assert!(!has(cx, "issue-editor-root"));
    assert!(has(cx, "issue-inline-save"));
    cx.update(|w, cx| {
        w.input("Urgent", cx);
        w.press("enter", cx);
    });
    render(cx);
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(patch_bodies(&fixture), [json!({"version":1,"priority":1})]);
}

#[gpui_kit::test]
fn date_and_description_entry_points_focus_their_input_and_cancel_keeps_the_issue(
    cx: &mut TestAppContext,
) {
    let (fixture, board, cx) = opened(cx);
    cx.update(|w, cx| {
        w.click("issue-property-edit:Due date", cx);
        w.input("2026-12-04", cx);
    });
    render(cx);
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"dueDate":"2026-12-04"})]
    );
    settle_board(&board, cx, |b| b.can_edit_issue());
    render(cx);
    cx.update(|w, cx| w.click("issue-description-edit", cx));
    render(cx);
    cx.update(|w, cx| w.press("escape", cx));
    render(cx);
    assert!(has(cx, "issues-detail"));
    assert_eq!(patch_bodies(&fixture).len(), 1);
}

#[gpui_kit::test]
fn milestone_picker_loads_project_choices_and_saves_only_the_link(cx: &mut TestAppContext) {
    let (fixture, board, cx) = opened(cx);
    cx.update(|w, cx| w.click("issue-property-edit:Milestone", cx));
    super::properties::wait_for_option(cx, 0);
    super::properties::wait_for_option(cx, 1);
    cx.update(|w, cx| {
        w.input("First delivery", cx);
        w.press("enter", cx);
    });
    render(cx);
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"milestoneId":"milestone-1"})]
    );
}

#[gpui_kit::test]
fn tracker_state_is_derived_and_archived_issues_offer_no_edits(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = opened(cx);
    board.update(cx, |b, cx| b.open_detail("SUB-1".into(), cx));
    settle_board(&board, cx, |b| b.can_edit_issue());
    render(cx);
    assert!(has(cx, "issue-property-edit:State"));
    assert!(has(cx, "issue-property-edit:Priority"));
    board.update(cx, |b, cx| b.open_detail("SUB-3".into(), cx));
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .and_then(|s| s.detail())
            .is_some_and(|d| d.fields.archived_at.is_some())
    });
    render(cx);
    assert!(!has(cx, "issue-property-edit:Priority"));
}

#[gpui_kit::test]
fn an_empty_cycle_can_be_assigned_through_the_full_editor(cx: &mut TestAppContext) {
    let (fixture, board, cx) = opened(cx);
    assert!(!has(cx, "issue-property-edit:Cycle"));
    cx.update(|w, cx| w.press("e", cx));
    render(cx);
    cx.update(|w, cx| w.click("issue-edit-cycle", cx));
    super::properties::wait_for_option(cx, 1);
    cx.update(|w, cx| {
        w.input("Cycle 1", cx);
        w.press("enter", cx);
    });
    render(cx);
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"cycleId":"cycle-1"})]
    );
}

#[gpui_kit::test]
fn a_finished_current_cycle_stays_checked_and_selecting_it_does_not_write(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        if r.path == "/api/v1/issues/SUB-2" || r.path == "/api/v1/issues/2" {
            let mut detail: serde_json::Value =
                serde_json::from_str(&issue("Historic cycle", 1)).unwrap();
            detail["cycleId"] = json!("cycle-3");
            detail["cycle"] = json!({"id":"cycle-3","number":3,"state":"finished"});
            return Some((200, detail.to_string()));
        }
        r.path.starts_with("/api/v1/issue-cycles?").then(|| {
            (
                200,
                json!({"items":[{"id":"cycle-4","number":4,"state":"current"}],"nextCursor":null})
                    .to_string(),
            )
        })
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    super::open_page(&board, cx);
    cx.update(|w, cx| w.click("issue-property-edit:Cycle", cx));
    super::properties::wait_for_option(cx, 2);
    cx.update(|w, cx| {
        w.input("Cycle 3", cx);
        w.press("enter", cx);
    });
    render(cx);
    assert!(crate::tests::labelled(cx, "Cycle: Cycle 3 · finished"));
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    render(cx);
    assert!(patch_bodies(&fixture).is_empty());
}

#[gpui_kit::test]
fn an_editor_receives_a_milestone_name_that_arrives_after_opening(cx: &mut TestAppContext) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let held = Arc::new(AtomicBool::new(true));
    let loading = held.clone();
    let fixture = offline_with(move |r| {
        if r.path == "/api/v1/issues/SUB-2" || r.path == "/api/v1/issues/2" {
            let mut detail: serde_json::Value =
                serde_json::from_str(&issue("Late name", 1)).unwrap();
            detail["milestoneId"] = json!("milestone-1");
            return Some((200, detail.to_string()));
        }
        if r.path.starts_with("/api/v1/projects/SUB/milestones?") {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while loading.load(Ordering::SeqCst) && std::time::Instant::now() < end {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            return Some((
                200,
                json!({"items":[{"id":"milestone-1","name":"First delivery"}],"nextCursor":null})
                    .to_string(),
            ));
        }
        None
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    super::open_page(&board, cx);
    cx.update(|w, cx| w.press("e", cx));
    render(cx);
    assert!(crate::tests::labelled(cx, "Milestone: Current milestone"));
    held.store(false, Ordering::SeqCst);
    settle_board(&board, cx, |b| {
        b.issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| s.milestone_name() == Some("First delivery"))
    });
    render(cx);
    assert!(crate::tests::labelled(cx, "Milestone: First delivery"));
    assert!(patch_bodies(&fixture).is_empty());
}

#[gpui_kit::test]
fn cancelling_an_inline_picker_restores_the_property_without_a_patch(cx: &mut TestAppContext) {
    let (fixture, board, cx) = opened(cx);
    cx.update(|w, cx| w.click("issue-property-edit:Priority", cx));
    super::properties::wait_for_option(cx, 0);
    cx.update(|w, cx| {
        w.input("Urgent", cx);
        w.press("enter", cx);
    });
    render(cx);
    cx.update(|w, cx| w.click("issue-inline-cancel", cx));
    render(cx);
    assert!(cx.read(|cx| board.read(cx).editor.is_none()));
    assert!(has(cx, "issue-property-edit:Priority"));
    assert!(patch_bodies(&fixture).is_empty());
}

#[gpui_kit::test]
fn lost_access_clears_an_inline_draft_and_the_issue_content(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = opened(cx);
    cx.update(|w, cx| w.click("issue-property-edit:Due date", cx));
    assert!(board.read_with(cx, |b, _| b.editor.is_some()));
    board.update(cx, |b, cx| {
        b.set_access(
            crate::scope::Access::Failed(crate::listing::Failure::Edge),
            cx,
        )
    });
    assert!(board.read_with(cx, |b, _| b.editor.is_none()
        && b.project_move.is_none()
        && b.issue_view.snapshot.is_none()));
    render(cx);
    assert!(!has(cx, "issue-inline-editor"));
}
