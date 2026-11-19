//! Pointer activation of a selectable IRC issue reference.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ClipboardItem, TestAppContext};
use subspace_framework::dashboard::DashboardEvent;

use super::open_board;

#[gpui_kit::test]
fn secondary_click_copies_the_displayed_issue_text_and_primary_click_opens_issues(
    cx: &mut TestAppContext,
) {
    let (board, _, _, cx) = open_board(cx, true, "TST-13 is still loading");
    let destination = Rc::new(RefCell::new(None));
    let _subscription = cx.update(|window, cx| {
        let received = destination.clone();
        let subscription = cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Navigate(link) = event {
                *received.borrow_mut() = Some(link.url.clone());
            }
        });
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        window.render_frame(cx);
        window.right_click("irc-reference:Some(0..36)", cx);
        subscription
    });
    cx.run_until_parked();
    assert!(destination.borrow().is_none());
    assert!(cx.opened_url().is_none());
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert_eq!(menu.find(0usize).label(), Some("Copy"));
        assert_eq!(menu.find(1usize).label(), Some("Select all"));
        assert_eq!(menu.find(2usize).label(), Some("Add to chat"));
        menu.click(0usize, cx);
    });
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("TST-13")
    );
    cx.update(|window, cx| window.click("irc-reference:Some(0..36)", cx));
    cx.run_until_parked();
    assert_eq!(
        destination.borrow().as_deref(),
        Some("https://api.example.test/api/v1/issues/TST-13?profile=dev")
    );
    assert!(cx.opened_url().is_none());
}
