use super::{
    board_with_hub, has, issue, issue_edit_route, offline_with, open_editor, patch_bodies, render,
    settle_board,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

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
    assert!(!has(cx, "issue-edit-owner"));
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
    let fixture = offline_with(issue_edit_route);
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

#[gpui_kit::test]
fn the_default_markdown_editor_saves_a_formatted_description(cx: &mut TestAppContext) {
    let fixture = offline_with(issue_edit_route);
    let (board, cx) = open_editor(&fixture, cx);
    cx.update(|w, cx| {
        assert_eq!(
            w.find(("issue-description-mode", 0_usize)).checked(),
            Some(true)
        );
        w.click("issue-description-mode:formatted", cx);
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.input("A formatted outcome", cx);
        w.press(subspace_framework::dashboard::secondary!("a"), cx);
        w.press(subspace_framework::dashboard::secondary!("b"), cx);
        let editing = w.focused(cx).expect("Markdown editor has focus");
        w.press("tab", cx);
        assert_ne!(w.focused(cx).as_ref(), Some(&editing));
        w.click("issue-description-mode:formatted", cx);
        assert_eq!(w.focused(cx), Some(editing));
    });
    render(cx);
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version": 1, "description": "**A formatted outcome**"})]
    );
}

pub(super) fn tab_to_visible_field(cx: &mut VisualTestContext, id: &'static str) {
    // The editor toolbar adds Tab stops before the issue properties.
    for _ in 0..32 {
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

fn owner_write_fixture() -> super::Offline {
    let persisted = AtomicBool::new(false);
    offline_with(
        move |r| match (r.method.as_str(), r.path.split('?').next().unwrap()) {
            ("GET", "/api/v1/users") => Some((
                200,
                json!({"items":[
                    {"id":"agent-fixture","displayName":"Orbit Fixture","kind":"agent"},
                    {"id":"human-fixture","displayName":"Rosa Fixture","kind":"human"}
                ],"nextCursor":null})
                .to_string(),
            )),
            ("GET" | "PATCH", "/api/v1/issues/SUB-2" | "/api/v1/issues/2") => {
                if r.method == "PATCH" {
                    persisted.store(true, Ordering::Relaxed);
                }
                let saved = persisted.load(Ordering::Relaxed);
                let mut value: serde_json::Value =
                    serde_json::from_str(&issue("Owner fixture", if saved { 2 } else { 1 }))
                        .unwrap();
                value["owner"] = if saved {
                    json!({"userId":"human-fixture","displayName":"Rosa Fixture","kind":"human","isFormerMember":false})
                } else {
                    json!({"userId":"former-fixture","displayName":"Former Fixture","kind":"human","isFormerMember":true})
                };
                Some((200, value.to_string()))
            }
            _ => None,
        },
    )
}

#[gpui_kit::test]
fn owner_picker_offers_humans_and_saves_only_the_versioned_owner(cx: &mut TestAppContext) {
    let fixture = owner_write_fixture();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |b| b.can_create_issue());
    super::open_page(&board, cx);
    assert_eq!(
        crate::tests::label(cx, "issue-property:Owner").as_deref(),
        Some("Owner: Former Fixture (former member)"),
    );
    cx.update(|w, cx| w.press("e", cx));
    render(cx);
    assert!(has(cx, "issue-edit-owner"));
    cx.update(|w, cx| w.press("escape", cx));
    render(cx);
    cx.update(|w, cx| w.click("issue-property-edit:Owner", cx));
    wait_for_option(cx, 0);
    cx.update(|w, cx| {
        assert!(w.try_find(("menu-option", 1_usize)).is_none());
        w.press("enter", cx);
    });
    render(cx);
    cx.update(|w, cx| w.click("issue-inline-save", cx));
    settle_board(&board, cx, |b| {
        b.editor.is_none()
            && b.issue_view
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.detail())
                .and_then(|detail| detail.fields.owner.as_ref())
                .is_some_and(|owner| owner.user_id == "human-fixture")
    });
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"ownerId":"human-fixture"})]
    );
    render(cx);
    assert_eq!(
        crate::tests::label(cx, "issue-property:Owner").as_deref(),
        Some("Owner: Rosa Fixture"),
    );
}
