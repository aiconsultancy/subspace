//! Keyboard and assistive focus in the decision panel: Tab reveals clipped
//! controls, keys scroll the reasoning, and the issue page stays inert.
use super::super::offline::attach_hub;
use super::super::{mount_at, press};
use super::{
    LONG_LINKS, NEW, OLD, choose, has, long_fixture, normal_fixture, open, open_issue, open_row,
    render, settle_detail, settle_page,
};
use crate::Issues;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, FocusHandle, ScrollHandle, TestAppContext, VisualTestContext, px};

const EVIDENCE: &str = "decision-evidence-reference";

fn panel_scroll(board: &Entity<Issues>, cx: &mut VisualTestContext) -> ScrollHandle {
    cx.update(|_, cx| board.read(cx).decision.as_ref().unwrap().scroll.clone())
}

/// The panel controls in Tab order after Close, as their reveal scopes.
fn control_scopes() -> Vec<String> {
    let supersedes = ["decision-supersedes:", OLD].concat();
    let issues = (1..=LONG_LINKS).map(|n| format!("decision-issue:LNK-{n}"));
    [EVIDENCE.to_owned(), supersedes]
        .into_iter()
        .chain(issues)
        .map(|key| key + ":scope")
        .collect()
}

/// Whether the scope's control lies wholly inside the panel body's viewport.
fn shown(scroll: &ScrollHandle, scope: &str, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| {
        let control = window.find(scope.to_owned()).bounds();
        let viewport = scroll.bounds();
        control.top() >= viewport.top() - px(0.5) && control.bottom() <= viewport.bottom() + px(0.5)
    })
}

/// A key, then the frame in which a newly focused control is revealed.
fn press_and_settle(cx: &mut VisualTestContext, key: &str) {
    press(cx, key);
    render(cx);
}

fn tab_through(scroll: &ScrollHandle, scopes: &[&String], key: &str, cx: &mut VisualTestContext) {
    for scope in scopes {
        press_and_settle(cx, key);
        let focused = cx.update(|window, _| window.find(scope.to_string()).focused());
        assert_eq!(focused, Some(true), "{scope} should have focus");
        assert!(shown(scroll, scope, cx), "{scope} should be in view");
    }
}

fn offset(scroll: &ScrollHandle) -> f32 {
    -f32::from(scroll.offset().y)
}

fn open_long_decision(cx: &mut TestAppContext) -> (Entity<Issues>, &mut VisualTestContext) {
    let fixture = long_fixture();
    let (board, cx) = mount_at(cx, 700.);
    attach_hub(&fixture, &board, cx);
    open_issue(&board, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    (board, cx)
}

fn assert_keys_scroll_the_body(scroll: &ScrollHandle, cx: &mut VisualTestContext) {
    press_and_settle(cx, "home");
    assert_eq!(offset(scroll), 0.);
    press_and_settle(cx, "down");
    assert_eq!(offset(scroll), 40.);
    press_and_settle(cx, "pagedown");
    assert!(offset(scroll) > 140.);
    press_and_settle(cx, "up");
    let after_up = offset(scroll);
    press_and_settle(cx, "pageup");
    assert!(offset(scroll) < after_up);
    press_and_settle(cx, "end");
    assert_eq!(offset(scroll), f32::from(scroll.max_offset().y));
}

#[gpui_kit::test]
fn tab_reveals_each_clipped_control_and_keys_scroll_the_reasoning(cx: &mut TestAppContext) {
    let (board, cx) = open_long_decision(cx);
    let scroll = panel_scroll(&board, cx);
    let scopes = control_scopes();
    let forward: Vec<_> = scopes.iter().collect();
    let evidence = &scopes[0];
    assert!(!shown(&scroll, evidence, cx), "the evidence starts clipped");

    tab_through(&scroll, &forward, "tab", cx);
    press_and_settle(cx, "tab");
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

    // From Close, the keys scroll the reasoning itself.
    assert_keys_scroll_the_body(&scroll, cx);
    assert!(
        !shown(&scroll, evidence, cx),
        "the evidence is above the view"
    );

    // Shift+Tab from the top reaches the last control, then each earlier one,
    // ending above the viewport at the evidence link.
    press_and_settle(cx, "home");
    let backward: Vec<_> = scopes.iter().rev().collect();
    tab_through(&scroll, &backward, "shift-tab", cx);
}

/// The selected Activity tab's handle, which GPUI Kit's tab list owns, found
/// by focusing it.
fn comments_tab(board: &Entity<Issues>, cx: &mut VisualTestContext) -> FocusHandle {
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            let focus = board.issue_view.timeline_focus.as_ref().unwrap();
            focus.focus_selected(window, cx);
        });
    });
    // The frames until the Tab order has the selected tab, then its handle
    // before anything reacts to the focus.
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        render(cx);
    }
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        let tabs = &board
            .read(cx)
            .issue_view
            .timeline_focus
            .as_ref()
            .unwrap()
            .tabs;
        assert!(
            tabs.contains_focused(window, cx),
            "an Activity tab has focus"
        );
        window.focused(cx).unwrap()
    })
}

#[gpui_kit::test]
fn issue_controls_behind_the_panel_cannot_keep_focus_until_it_closes(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    open_row(&board, cx);
    settle_detail(&board, cx, NEW);
    let comments = comments_tab(&board, cx);
    // As an assistive technology's focus action would.
    cx.update(|window, cx| comments.focus(window, cx));
    render(cx);
    render(cx);
    cx.update(|window, cx| {
        let panel = board.read(cx).decision.as_ref().unwrap();
        assert!(!comments.is_focused(window));
        assert!(panel.trap.contains_focused(window, cx));
        assert!(panel.close.is_focused(window));
    });

    press(cx, "escape");
    assert!(!has(cx, "decision-panel"));
    cx.update(|window, cx| comments.focus(window, cx));
    render(cx);
    cx.update(|window, _| assert!(comments.is_focused(window)));
}
