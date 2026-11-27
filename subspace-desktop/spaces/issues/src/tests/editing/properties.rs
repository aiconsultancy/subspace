use super::{
    board_with_hub, has, issue, offline_with, open_editor, patch_bodies, render, settle_board,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

#[gpui_kit::test]
fn property_pickers_use_the_api_and_tracker_controls_refuse_direct_values(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        match r.path.split('?').next().unwrap() {
        "/api/v1/users" => Some((200, json!({"items":[{"id":"agent-fixture","displayName":"Orbit Fixture","kind":"agent"}],"nextCursor":null}).to_string())),
        "/api/v1/projects/SUB" => Some((200, json!({"estimateScale":"tShirt"}).to_string())),
        _ => None,
    }
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    render(cx);
    cx.update(|w, cx| w.press("c", cx));
    render(cx);
    cx.update(|w, cx| w.click("issue-edit-assignee", cx));
    settle_board(&board, cx, |b| b.menu.is_some());
    wait_for_option(cx, 1);
    cx.update(|w, cx| {
        w.input("Orbit", cx);
        w.press("enter", cx);
    });
    render(cx);
    assert!(crate::tests::labelled(
        cx,
        "Assignee: Orbit Fixture (agent)"
    ));
    cx.update(|w, cx| w.click("issue-edit-estimate", cx));
    wait_for_option(cx, 5);
    cx.update(|w, cx| {
        w.input("XL", cx);
        w.press("enter", cx);
    });
    render(cx);
    assert!(crate::tests::labelled(cx, "Estimate: XL"));
    cx.update(|w, cx| w.click("issue-edit-type", cx));
    render(cx);
    cx.update(|w, cx| {
        w.input("Tracker", cx);
        w.press("enter", cx);
    });
    render(cx);
    cx.update(|w, cx| {
        w.click("issue-edit-state", cx);
        w.click("issue-edit-estimate", cx);
    });
    render(cx);
    assert!(!has(cx, "menu"));
    assert!(!fixture.mock.requests().iter().any(|r| r.method == "POST"));
}

pub(super) fn wait_for_option(cx: &mut VisualTestContext, ix: usize) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        render(cx);
        if cx.update(|w, _| w.try_find(("menu-option", ix)).is_some()) {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "missing option {ix}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[gpui_kit::test]
fn description_and_due_edits_send_sparse_fields_and_keep_mentions(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| match (r.method.as_str(), r.path.as_str()) {
        ("PATCH", "/api/v1/issues/2") => Some((200, issue("Original", 2))),
        ("GET", "/api/v1/issues/SUB-2" | "/api/v1/issues/2") => Some((200, issue("Original", 1))),
        _ => None,
    });
    let (board, cx) = open_editor(&fixture, cx);
    let description =
        "## Outcome\n\nKeep @[Ada Fixture](member:12345678-1234-1234-1234-123456789abc)";
    cx.update(|w, cx| w.click(("issue-description-mode", 1_usize), cx));
    render(cx);
    assert!(!cx.update(|_, cx| board.read(cx).issue_editor_dirty(cx)));
    cx.update(|w, cx| {
        assert_eq!(
            w.find(("issue-description-mode", 1_usize)).checked(),
            Some(true)
        );
        w.within("issue-edit-description").click("input-state", cx);
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.input(description, cx);
        w.input("[", cx);
        w.press("tab", cx);
        assert_eq!(
            w.within("issue-edit-description")
                .find("input-state")
                .focused(),
            Some(false)
        );
    });
    render(cx);
    cx.update(|w, cx| w.click(("issue-description-mode", 0_usize), cx));
    render(cx);
    cx.update(|w, _| {
        assert_eq!(
            w.find(("issue-description-mode", 0_usize)).checked(),
            Some(true)
        )
    });
    tab_to_visible_field(cx, "issue-edit-due");
    cx.update(|w, cx| {
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.input("2026-12-01", cx);
    });
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"description":format!("{description}["),"dueDate":"2026-12-01"})]
    );
}

pub(super) fn tab_to_visible_field(cx: &mut VisualTestContext, id: &'static str) {
    for _ in 0..16 {
        if cx.update(|window, _| window.find(id).focused() == Some(true)) {
            render(cx);
            cx.update(|window, _| {
                assert!(window.find(id).visible(), "focused field {id} is hidden")
            });
            return;
        }
        cx.update(|window, cx| window.press("tab", cx));
        cx.run_until_parked();
    }
    panic!("Tab did not reach {id}");
}
