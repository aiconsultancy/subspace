use super::properties::{tab_to_visible_field, wait_for_option};
use super::{
    board_with_hub, has, issue, offline_with, open_editor, open_page, patch_bodies, render, row,
    settle_board,
};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::json;
use std::time::Duration;
use subspace_framework::client_test_support::Request;

const PICKER: &str = "issue-parent-picker";

#[gpui_kit::test]
fn full_editor_searches_and_pages_trackers_and_saves_the_selection(cx: &mut TestAppContext) {
    let fixture = offline_with(search_route);
    let (board, cx) = open_editor(&fixture, cx);
    tab_to_visible_field(cx, PICKER);
    cx.update(|w, cx| w.press("space", cx));
    wait_for_option(cx, 0);
    cx.update(|w, cx| w.input("Roadmap", cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(250));
    settle_board(&board, cx, |b| {
        b.test_parent_options() == ["No parent", "Load more trackers"]
    });
    wait_for_option(cx, 1);
    cx.update(|w, cx| w.press("down", cx));
    render(cx);
    cx.update(|w, cx| w.press("enter", cx));
    settle_board(&board, cx, |b| {
        b.test_parent_options() == ["No parent", "SUB-1 · Roadmap · In Progress"]
    });
    wait_for_option(cx, 1);
    assert!(cx.update(|w, _| w.try_find(("menu-option", 2_usize)).is_none()));
    cx.update(|w, cx| w.press("enter", cx));
    render(cx);
    assert!(!has(cx, "menu"));
    assert!(crate::tests::labelled(cx, "Parent: SUB-1"));
    cx.update(|w, cx| w.press(subspace_framework::dashboard::secondary!("enter"), cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"parentId":"SUB-1"})]
    );
    assert!(
        fixture
            .mock
            .requests()
            .iter()
            .any(|r| r.path.contains("q=Roadmap")
                && r.path.contains("cursor=fixture-next")
                && r.path.contains("parentFor=2"))
    );
}

fn search_route(r: &Request) -> Option<(u16, String)> {
    if r.path.contains("type=tracker") {
        let mut tracker = row("1", "Roadmap");
        tracker["type"] = json!("tracker");
        let value = if r.path.contains("cursor=fixture-next") {
            json!({"items":[tracker],"nextCursor":null})
        } else if r.path.contains("q=Roadmap") {
            json!({"items":[],"nextCursor":"fixture-next"})
        } else {
            json!({"items":[],"nextCursor":null})
        };
        return Some((200, value.to_string()));
    }
    match (r.method.as_str(), r.path.as_str()) {
        ("PATCH", "/api/v1/issues/2") => Some((200, issue("Original", 2))),
        ("GET", "/api/v1/issues/SUB-2" | "/api/v1/issues/2") => Some((200, issue("Original", 1))),
        _ => None,
    }
}

#[gpui_kit::test]
fn inline_tracker_parent_can_be_cleared(cx: &mut TestAppContext) {
    let fixture = offline_with(|r| {
        let (status, body) = search_route(r)?;
        if r.path != "/api/v1/issues/SUB-2" && r.path != "/api/v1/issues/2" {
            return Some((status, body));
        }
        let mut value: serde_json::Value = serde_json::from_str(&body).unwrap();
        value["type"] = json!("tracker");
        if r.method == "GET" {
            value["parentId"] = json!("1");
            value["parent"] = row("1", "Roadmap");
            value["parent"]["archived"] = json!(false);
        }
        Some((status, value.to_string()))
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    open_page(&board, cx);
    cx.update(|w, cx| w.click("issue-property-edit:Parent", cx));
    wait_for_option(cx, 0);
    cx.update(|w, cx| w.press("enter", cx));
    render(cx);
    assert!(!has(cx, "menu"));
    assert!(crate::tests::labelled(cx, "Parent: No parent"));
    cx.update(|w, cx| w.click("issue-inline-save", cx));
    settle_board(&board, cx, |b| b.editor.is_none());
    assert_eq!(
        patch_bodies(&fixture),
        [json!({"version":1,"parentId":null})]
    );
}
