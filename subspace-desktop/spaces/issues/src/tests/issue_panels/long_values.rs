//! Long values stay inside the Properties panel and the page column: a label
//! name with no spaces is cut at the panel edge, not clipped past it.
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px};
use serde_json::json;
use subspace_framework::dashboard::Dashboard as _;

use crate::Issues;
use crate::tests::offline::{Offline, fixture_hub, offline_with, settle_board};
use crate::tests::{mount_at, render};

/// A generated end-to-end label: 68 characters and no break opportunity.
const LONG_LABEL: &str = "e2e-f0a573fcdfe94c24a3ec9b79afb9f91d-planning-views-regression-suite";

fn long_label_fixture() -> Offline {
    offline_with(|request| {
        if !request.path.ends_with("/issues/SUB-1") {
            return None;
        }
        let mut detail = crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
        detail["identifier"] = json!("SUB-1");
        detail["labels"] = json!([
            {"id": "label-short", "name": "Fixture", "color": "#6C8EEF", "state": "active"},
            {"id": "label-long", "name": LONG_LABEL, "color": "#4C7899", "state": "active"},
        ]);
        Some((200, detail.to_string()))
    })
}

fn opened_at(cx: &mut TestAppContext, width: f32) -> (Entity<Issues>, &mut VisualTestContext) {
    let fixture = long_label_fixture();
    let (hub, address) = fixture_hub(&fixture);
    let (board, cx) = mount_at(cx, width);
    cx.update(|window, cx| {
        crate::link::install(cx, hub, Some(address));
        board.update(cx, |board, cx| {
            board.activate(window, cx);
            board.open_detail("SUB-1".into(), cx);
        });
    });
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| s.detail().is_some())
    });
    render(cx);
    render(cx);
    (board, cx)
}

/// Every property row, and the long label's chip, end inside the panel.
fn assert_inside_the_panel(cx: &mut VisualTestContext, width: f32) {
    cx.update(|window, _| {
        let panel = window.find("issue-properties").bounds();
        let labels = window.find("issue-property-labels").bounds();
        let long = window.find("issue-label:label-long").bounds();
        let short = window.find("issue-label:label-short").bounds();
        assert!(panel.right() <= px(width), "{panel:?} past the window");
        assert!(labels.right() <= panel.right(), "{labels:?} past {panel:?}");
        assert!(long.right() <= labels.right(), "{long:?} past {labels:?}");
        assert!(long.size.width <= labels.size.width);
        assert!(short.right() <= labels.right());
        for label in ["State", "Labels", "Project", "Assignee", "Type"] {
            let row = window.find(format!("issue-property:{label}")).bounds();
            assert!(
                row.right() <= panel.right(),
                "{label} row {row:?} past {panel:?}"
            );
        }
    });
}

#[gpui_kit::test]
fn a_long_label_is_cut_inside_the_properties_panel(cx: &mut TestAppContext) {
    let (_board, cx) = opened_at(cx, 1280.);
    assert_inside_the_panel(cx, 1280.);
}

#[gpui_kit::test]
fn a_long_label_is_cut_inside_the_narrowest_two_column_page(cx: &mut TestAppContext) {
    let (_board, cx) = opened_at(cx, 1000.);
    assert_inside_the_panel(cx, 1000.);
}

#[gpui_kit::test]
fn a_long_label_is_cut_inside_the_stacked_page(cx: &mut TestAppContext) {
    let (_board, cx) = opened_at(cx, 720.);
    assert_inside_the_panel(cx, 720.);
}
