//! Pointer activation, retained selection, measured animation and tail following.
use super::open_board;
use crate::{Irc, conversation::Conversation, fixtures::channel};
use gpui_kit::{Entity, Pixels, TestAppContext, VisualTestContext, px, test::TestWindowExt as _};
use std::{cell::RefCell, rc::Rc, time::Duration};
use subspace_framework::dashboard::{DashboardEvent, animated_collapsible::DISCLOSURE_DURATION};

const FIRST: &str = "irc-line:2:issue-summary:TST-12:toggle";
const SECOND: &str = "irc-line:2:issue-summary:TST-18:toggle";
const PANEL: &str = "irc-line:2:summary-panel";

fn frame(cx: &mut VisualTestContext, elapsed: Duration) -> Pixels {
    cx.executor().advance_clock(elapsed);
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.refresh();
        window.render_frame(cx);
        window
            .try_find(PANEL)
            .map_or(px(0.), |panel| panel.bounds().size.height)
    })
}
fn conversation(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Entity<Conversation> {
    cx.update(|_, cx| {
        board.read(cx).dock.open[&channel("#subspace")]
            .entity
            .clone()
    })
}
fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    frame(cx, Duration::ZERO);
}

#[gpui_kit::test]
fn chevron_animates_open_and_closed_without_navigation_and_survives_redraw(
    cx: &mut TestAppContext,
) {
    let (board, _, _, cx) = open_board(cx, true, "TST-12 and TST-18");
    let opened = Rc::new(RefCell::new(Vec::new()));
    let sink = opened.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Navigate(link) = event {
                sink.borrow_mut().push(link.url.clone());
            }
        })
    });
    click(cx, FIRST);
    let middle = frame(cx, DISCLOSURE_DURATION / 2);
    let full = frame(cx, DISCLOSURE_DURATION);
    assert!(middle > px(0.) && middle < full, "{middle:?} vs {full:?}");
    assert_eq!(frame(cx, Duration::ZERO), full, "redraw retains expansion");
    cx.update(|window, _| assert_eq!(window.find(FIRST).label(), Some("Hide summary of TST-12")));
    click(cx, FIRST);
    let closing = frame(cx, DISCLOSURE_DURATION / 2);
    assert!(closing > px(0.) && closing < full);
    assert_eq!(frame(cx, DISCLOSURE_DURATION), px(0.));
    assert!(opened.borrow().is_empty());
}

#[gpui_kit::test]
fn another_chip_switches_the_single_panel_and_null_has_a_quiet_fallback(cx: &mut TestAppContext) {
    let (board, _, _, cx) = open_board(cx, true, "TST-12 and TST-18");
    click(cx, FIRST);
    frame(cx, DISCLOSURE_DURATION);
    click(cx, SECOND);
    frame(cx, DISCLOSURE_DURATION);
    let view = conversation(&board, cx);
    cx.update(|window, cx| {
        assert_eq!(window.find_all(PANEL).len(), 1);
        assert_eq!(
            view.read(cx).summaries.panel("message:2").unwrap().issue,
            "TST-18"
        );
        assert_eq!(window.find(FIRST).label(), Some("Show summary of TST-12"));
        assert_eq!(window.find(SECOND).label(), Some("Hide summary of TST-18"));
        assert_eq!(window.find(PANEL).label(), Some("No summary yet"));
        assert!(window.find(PANEL).visible());
    });
}

#[gpui_kit::test]
fn reduced_motion_finishes_both_directions_at_once(cx: &mut TestAppContext) {
    let (_, _, _, cx) = open_board(cx, true, "TST-12");
    cx.update(|_, cx| cx.set_reduce_motion(true));
    click(cx, FIRST);
    assert!(frame(cx, Duration::ZERO) > px(0.));
    click(cx, FIRST);
    assert_eq!(frame(cx, Duration::ZERO), px(0.));
}

#[gpui_kit::test]
fn a_scrollable_transcript_keeps_its_bottom_while_the_summary_changes_height(
    cx: &mut TestAppContext,
) {
    const LINES: usize = 80;
    let text = format!("{}TST-12", "A fictional transcript line.\n".repeat(LINES));
    let (board, _, _, cx) = open_board(cx, true, &text);
    let view = conversation(&board, cx);
    let bottom = cx.update(|window, cx| {
        assert!(view.read(cx).is_following_summary_tail(cx));
        window.find("irc-line:2:row").bounds().bottom()
    });
    for toggle in [FIRST, FIRST] {
        cx.update(|window, cx| {
            window.click(toggle, cx);
            assert!(view.read(cx).is_following_summary_tail(cx));
            assert_eq!(window.find("irc-line:2:row").bounds().bottom(), bottom);
        });
        for elapsed in [Duration::ZERO, DISCLOSURE_DURATION / 2, DISCLOSURE_DURATION] {
            frame(cx, elapsed);
            cx.update(|window, cx| {
                assert!(view.read(cx).is_following_summary_tail(cx));
                assert_eq!(window.find("irc-line:2:row").bounds().bottom(), bottom);
            });
        }
    }
}

#[gpui_kit::test]
fn collapsed_summary_has_neither_content_nor_accessible_label(cx: &mut TestAppContext) {
    let (_, _, _, cx) = open_board(cx, true, "TST-12");
    cx.update(|window, _| assert!(window.try_find(PANEL).is_none()));
    click(cx, FIRST);
    frame(cx, DISCLOSURE_DURATION);
    let text = cx.update(|window, _| window.find(PANEL).label().unwrap().to_owned());
    click(cx, FIRST);
    frame(cx, DISCLOSURE_DURATION / 2);
    cx.update(|window, _| assert_eq!(window.find(PANEL).label(), Some(text.as_str())));
    frame(cx, DISCLOSURE_DURATION);
    cx.update(|window, _| assert!(window.try_find(PANEL).is_none()));
}
