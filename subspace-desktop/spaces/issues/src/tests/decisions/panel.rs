use super::super::offline::{attach_hub, wait_for};
use super::super::{mount_at, press};
use super::{
    ALL, NEW, OLD, ROW, choose, click, has, label, normal_fixture, open, open_issue, open_row,
    render, reveal, reveal_issue, settle_board, settle_detail, settle_page, switched,
};
use crate::{Issues, detail::timeline::TimelineMode};
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px};
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};
use subspace_framework::dashboard::{Dashboard as _, Modal, Tokens, panel::clamp_width};

const CLOSE: &str = "decision-panel-close";

#[gpui_kit::test]
fn keyboard_only_from_activity_to_decision_and_back(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    press(cx, "m");
    for key in ["right", "right", "enter"] {
        press(cx, key);
    }
    settle_page(&board, cx);
    press(cx, "tab");
    press(cx, "enter");
    settle_detail(&board, cx, NEW);
    assert_focus_trapped(&board, cx);
    press(cx, "escape");
    assert!(!has(cx, "decision-panel"));
    cx.update(|window, _| assert_eq!(window.find(ROW).focused(), Some(true)));
    press(cx, "escape");
    assert!(has(cx, "issues-tabs"));
}

#[gpui_kit::test]
fn escape_closes_a_text_viewer_before_the_decision_and_issue(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    let file = serde_json::from_value(serde_json::json!({
        "mediaId":"empty","kind":"file","fileName":"Empty.txt","contentType":"text/plain",
        "sizeBytes":0,"url":"/api/v1/issues/attachments/empty","sha256":"fixture"
    }))
    .unwrap();
    cx.update(|window, cx| board.update(cx, |board, cx| board.open_text(file, window, cx)));
    render(cx);
    assert!(has(cx, "attachment-viewer"));
    press(cx, "escape");
    assert!(!has(cx, "attachment-viewer"));
    assert!(has(cx, "decision-panel"));
    press(cx, "escape");
    assert!(!has(cx, "decision-panel"));
    assert!(has(cx, "issues-detail"));
    press(cx, "escape");
    assert!(has(cx, "issues-tabs"));
}

fn assert_focus_trapped(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    assert_eq!(
        cx.update(|_, cx| board.read(cx).modal(cx)),
        Some(Modal::Dialog)
    );
    cx.update(|window, cx| {
        assert!(
            board
                .read(cx)
                .decision
                .as_ref()
                .unwrap()
                .close
                .is_focused(window)
        );
    });
    for key in ["tab", "shift-tab", "shift-tab", "tab"] {
        press(cx, key);
        cx.update(|window, cx| {
            let panel = board.read(cx).decision.as_ref().unwrap();
            assert!(panel.trap.contains_focused(window, cx));
        });
    }
}

#[gpui_kit::test]
fn list_and_all_share_a_focus_trapped_panel_and_escape_returns_to_the_opener(
    cx: &mut TestAppContext,
) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    let detail_reads = || {
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.path.ends_with(NEW))
            .count()
    };
    for (mode, opener) in [(TimelineMode::Decisions, ROW), (TimelineMode::All, ALL)] {
        board.update(cx, |board, cx| board.show_timeline(mode, cx));
        if mode == TimelineMode::Decisions {
            settle_page(&board, cx);
        }
        render(cx);
        reveal_issue(&board, opener, cx);
        click(opener, cx);
        settle_detail(&board, cx, NEW);
        assert_eq!(
            label(cx, "decision-panel").as_deref(),
            Some("Decision Read decisions here")
        );
        assert_focus_trapped(&board, cx);
        click("issue-back", cx);
        assert!(has(cx, "decision-panel"));
        let reads = detail_reads();
        press(cx, "escape");
        assert!(!has(cx, "decision-panel"));
        assert!(has(cx, "issues-detail"));
        press(cx, "space");
        wait_for(|| detail_reads() == reads + 1);
        press(cx, "escape");
    }
}

#[gpui_kit::test]
fn closing_a_decision_without_an_opener_focuses_the_board(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            window.blur(cx);
            crate::decision::open(board, NEW, "Read decisions here", window, cx);
        });
    });
    settle_detail(&board, cx, NEW);
    press(cx, "escape");
    assert!(!has(cx, "decision-panel"));
    cx.update(|window, cx| assert!(board.read(cx).focus.is_focused(window)));
}

fn click_in_panel(board: &Entity<Issues>, id: &str, cx: &mut VisualTestContext) {
    let scroll = cx.update(|_, cx| board.read(cx).decision.as_ref().unwrap().scroll.clone());
    reveal(id, &scroll, cx);
    click(id, cx);
}

#[gpui_kit::test]
fn supersession_back_and_linked_issue_navigation_share_the_panel(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    assert_eq!(
        label(cx, "issue-decision:Made by").as_deref(),
        Some("Made by: A person")
    );
    assert_eq!(
        label(cx, "decision-superseded-by-stale").as_deref(),
        Some("Newer decision no longer available")
    );
    click_in_panel(&board, &format!("decision-supersedes:{OLD}"), cx);
    settle_detail(&board, cx, OLD);
    assert_eq!(
        label(cx, "issue-decision:Made by").as_deref(),
        Some("Made by: An agent")
    );
    assert_eq!(
        label(cx, "decision-panel-back").as_deref(),
        Some("Back to Read decisions here")
    );
    click("decision-panel-back", cx);
    settle_detail(&board, cx, NEW);
    assert!(!has(cx, "decision-panel-back"));
    assert_eq!(
        label(cx, "decision-issue:SUB-2").as_deref(),
        Some("Open issue SUB-2: Linked fixture issue")
    );
    click_in_panel(&board, "decision-issue:SUB-2", cx);
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| issue.identifier() == "SUB-2" && issue.detail().is_some())
    });
    render(cx);
    assert!(!has(cx, "decision-panel"));
    assert!(has(cx, "issue-comment-composer"));
}

#[gpui_kit::test]
fn supersede_and_back_reads_are_deferred_while_the_issue_reloads(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    for (control, id) in [
        (format!("decision-supersedes:{OLD}"), OLD),
        ("decision-panel-back".into(), NEW),
    ] {
        fixture.switches.hold_issue.store(true, Ordering::Relaxed);
        board.update(cx, |board, cx| board.reload_detail(cx));
        render(cx);
        if id == OLD {
            click_in_panel(&board, &control, cx);
        } else {
            click(&control, cx);
        }
        assert!(has(cx, "decision-panel-loading"));
        fixture.switches.hold_issue.store(false, Ordering::Relaxed);
        settle_detail(&board, cx, id);
        assert!(has(cx, "issue-decision:Made by"));
    }
}

#[gpui_kit::test]
fn an_evidence_link_shows_the_host_below_its_kind(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    assert_eq!(
        label(cx, "issue-decision:Evidence").as_deref(),
        Some("Evidence: Document or URL")
    );
    let scroll = cx.update(|_, cx| board.read(cx).decision.as_ref().unwrap().scroll.clone());
    reveal("issue-decision:Evidence", &scroll, cx);
    cx.update(|window, _| {
        let texts = gpui_kit::base::test_support::texts(window);
        assert!(texts.iter().any(|text| text.text() == "example.invalid"));
    });
}

#[gpui_kit::test]
fn a_bad_detail_can_retry_and_gone_has_no_retry(cx: &mut TestAppContext) {
    let code = Arc::new(AtomicU16::new(400));
    let fixture = switched(
        Arc::new(AtomicU16::new(200)),
        Arc::new(AtomicU16::new(200)),
        code.clone(),
    );
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    assert!(has(cx, "decision-panel-error"));
    assert!(has(cx, "decision-panel-retry"));
    code.store(404, Ordering::Relaxed);
    click("decision-panel-retry", cx);
    settle_detail(&board, cx, NEW);
    assert!(has(cx, "decision-panel-gone"));
    assert!(!has(cx, "decision-panel-retry"));
    assert!(has(cx, CLOSE));
}

#[gpui_kit::test]
fn at_700_px_tabs_wrap_below_activity_and_the_panel_width_is_clamped(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = mount_at(cx, 700.);
    attach_hub(&fixture, &board, cx);
    open_issue(&board, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    cx.update(|window, _| {
        let title = window.find("issue-timeline-title").bounds();
        let heading = window.find("issue-timeline-heading").bounds();
        for name in ["Comments", "All", "Decisions"] {
            let tab = window.find_by_label(name).bounds();
            assert!(tab.top() >= title.bottom());
            assert!(tab.left() >= heading.left() && tab.right() <= heading.right());
            assert!(tab.right() <= px(700.));
        }
    });
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    cx.update(|window, cx| {
        let width = clamp_width(f32::from(Tokens::current(cx).panel_width), 700.);
        assert_eq!(window.find("decision-panel").bounds().size.width, px(width));
    });
}
