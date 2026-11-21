//! Where the issue page's Back goes: the list, the view a link
//! came from, and the list again when that view is gone.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::{Entity, Subscription, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::{Dashboard as _, DashboardEvent, ReturnPoint};

use super::is_loaded;
use crate::Issues;
use crate::tests::offline::{Offline, board_with_hub, fixture_hub, offline, settle_board};
use crate::tests::{has, label, press, render, selected};

fn channel() -> ReturnPoint {
    ReturnPoint {
        board: "irc",
        label: "#discovery".into(),
        location: Some("{\"network\":\"libera\"}".into()),
    }
}

fn returns(
    board: &Entity<Issues>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Vec<ReturnPoint>>>, Subscription) {
    let seen: Rc<RefCell<Vec<ReturnPoint>>> = Rc::default();
    let sink = Rc::clone(&seen);
    let record = move |_: Entity<Issues>, event: &DashboardEvent, _: &mut gpui_kit::App| {
        let DashboardEvent::Return(point) = event else {
            return;
        };
        sink.borrow_mut().push(point.clone());
    };
    (seen, cx.update(|_, cx| cx.subscribe(board, record)))
}

fn link(board: &Entity<Issues>, identifier: &str, cx: &mut VisualTestContext) -> String {
    let origin = cx.update(|_, cx| {
        let hub = board.read(cx).hub.clone().unwrap();
        hub.irc_snapshot().api_origin().to_owned()
    });
    format!("{origin}/v1/issues/{identifier}?profile=subspace-api")
}

/// The shell makes the board current from `from`, then opens `identifier`
/// by link, as it does for an issue chip in a channel.
fn arrive_by_link(
    board: &Entity<Issues>,
    from: Option<ReturnPoint>,
    identifier: &str,
    cx: &mut VisualTestContext,
) {
    let url = link(board, identifier, cx);
    board.update(cx, |board, cx| {
        board.arrived_from(from, cx);
        assert!(board.open_link(&url, cx));
    });
    cx.update(|window, cx| board.update(cx, |board, cx| board.activate(window, cx)));
    settle_board(board, cx, |board| is_loaded(board, identifier));
    render(cx);
}

fn ready(cx: &mut TestAppContext) -> (Offline, Entity<Issues>, &mut VisualTestContext) {
    let fixture = offline();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    (fixture, board, cx)
}

#[gpui_kit::test]
fn from_the_list_back_returns_to_the_list_with_its_selection(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    render(cx);
    press(cx, "j");
    press(cx, "j");
    press(cx, "enter");
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    render(cx);
    assert_eq!(label(cx, "issue-back").as_deref(), Some("Back to issues"));
    press(cx, "escape");
    assert_eq!(selected(&board, cx).as_deref(), Some("issue:1"));
    assert!(has(cx, "issues-tabs"));
    assert!(seen.borrow().is_empty());
}

#[gpui_kit::test]
fn from_a_channel_back_names_it_and_returns_there(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    arrive_by_link(&board, Some(channel()), "SUB-1", cx);
    assert_eq!(
        label(cx, "issue-back").as_deref(),
        Some("Back to #discovery")
    );
    press(cx, "escape");
    assert_eq!(*seen.borrow(), [channel()]);
    // The list shows first: when the channel is gone, the shell stays
    // here and Back has landed on the list.
    render(cx);
    assert!(has(cx, "issues-tabs"));
    assert!(!has(cx, "issues-detail"));
}

#[gpui_kit::test]
fn closing_a_decision_keeps_the_channel_as_the_first_origin(cx: &mut TestAppContext) {
    use crate::tests::decisions;

    let fixture = decisions::normal_fixture();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    let (seen, _subscription) = returns(&board, cx);
    arrive_by_link(&board, Some(channel()), "SUB-1", cx);
    decisions::choose(&board, cx);
    decisions::settle_page(&board, cx);
    decisions::open_row(&board, cx);
    decisions::settle_detail(&board, cx, decisions::NEW);
    assert!(has(cx, "decision-panel"));

    press(cx, "escape");
    assert!(!has(cx, "decision-panel"));
    assert_eq!(
        label(cx, "issue-back").as_deref(),
        Some("Back to #discovery")
    );
    assert!(seen.borrow().is_empty());

    press(cx, "escape");
    assert_eq!(*seen.borrow(), [channel()]);
}

#[gpui_kit::test]
fn the_palette_back_names_its_destination_but_is_recent_by_a_static_key(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    arrive_by_link(&board, Some(channel()), "SUB-1", cx);
    let back = cx.update(|_, cx| {
        board
            .read(cx)
            .palette_commands()
            .into_iter()
            .find(|command| command.shortcut.as_deref() == Some("Esc"))
            .expect("the Back command")
    });
    assert_eq!(back.label.as_ref(), "Back to #discovery");
    assert_eq!(back.recent_key.as_deref(), Some("Back to issues"));
}

#[gpui_kit::test]
fn from_another_space_back_returns_to_the_last_view(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    let prs = ReturnPoint {
        board: "github",
        label: "PRs".into(),
        location: None,
    };
    arrive_by_link(&board, Some(prs.clone()), "SUB-1", cx);
    assert_eq!(label(cx, "issue-back").as_deref(), Some("Back to PRs"));
    cx.update(|window, cx| board.update(cx, |board, cx| board.go_back(window, cx)));
    assert_eq!(*seen.borrow(), [prs]);
}

#[gpui_kit::test]
fn with_no_view_before_back_returns_to_the_list(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    arrive_by_link(&board, None, "SUB-1", cx);
    assert_eq!(label(cx, "issue-back").as_deref(), Some("Back to issues"));
    press(cx, "escape");
    assert!(seen.borrow().is_empty());
    render(cx);
    assert!(has(cx, "issues-tabs"));
}

#[gpui_kit::test]
fn an_issue_opened_from_an_issue_keeps_the_first_origin(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    arrive_by_link(&board, Some(channel()), "SUB-1", cx);
    // A parent, sub-issue or relation opens in the same page.
    board.update(cx, |board, cx| board.open_detail("SUB-2".into(), cx));
    settle_board(&board, cx, |board| is_loaded(board, "SUB-2"));
    render(cx);
    assert_eq!(
        label(cx, "issue-back").as_deref(),
        Some("Back to #discovery")
    );
    // A link from inside the board, with no new arrival, keeps it too.
    let url = link(&board, "SUB-1", cx);
    board.update(cx, |board, cx| assert!(board.open_link(&url, cx)));
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    press(cx, "escape");
    assert_eq!(*seen.borrow(), [channel()]);
}

#[gpui_kit::test]
fn closing_the_page_forgets_the_origin(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    arrive_by_link(&board, Some(channel()), "SUB-1", cx);
    cx.update(|window, cx| board.update(cx, |board, cx| board.go_back(window, cx)));
    // Opened again from the list, Back goes to the list.
    board.update(cx, |board, cx| board.open_row("issue:1", cx));
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    render(cx);
    assert_eq!(label(cx, "issue-back").as_deref(), Some("Back to issues"));
    press(cx, "escape");
    assert_eq!(seen.borrow().len(), 1);
}

#[gpui_kit::test]
fn a_live_reread_of_the_open_issue_keeps_its_origin(cx: &mut TestAppContext) {
    let (_fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    arrive_by_link(&board, Some(channel()), "SUB-1", cx);
    // The hub reads the open issue again, as a live hint does.
    board.update(cx, |board, cx| board.reload_detail(cx));
    settle_board(&board, cx, |board| is_loaded(board, "SUB-1"));
    render(cx);
    assert_eq!(
        label(cx, "issue-back").as_deref(),
        Some("Back to #discovery")
    );
    press(cx, "escape");
    assert_eq!(*seen.borrow(), [channel()]);
}

#[gpui_kit::test]
fn a_new_hub_forgets_the_origin_from_the_last_account(cx: &mut TestAppContext) {
    let (fixture, board, cx) = ready(cx);
    let (seen, _subscription) = returns(&board, cx);
    let decision = ReturnPoint {
        board: "decisions",
        label: "Adopt a review rota".into(),
        location: Some("{\"number\":7}".into()),
    };
    arrive_by_link(&board, Some(decision), "SUB-1", cx);
    assert_eq!(
        label(cx, "issue-back").as_deref(),
        Some("Back to Adopt a review rota")
    );
    // The hub restarts for another account, as `poll_hub` does when the
    // signed-in account changes.
    let (hub, address) = fixture_hub(&fixture);
    cx.update(|_, cx| crate::link::install(cx, hub, Some(address)));
    board.update(cx, |board, cx| board.poll_hub(cx));
    cx.update(|_, cx| assert!(board.read(cx).back.origin().is_none()));
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    // An issue opened afterwards from this board, as from the palette, has
    // no arrival: Back goes to the list, not to the old account's decision.
    let url = link(&board, "SUB-2", cx);
    board.update(cx, |board, cx| assert!(board.open_link(&url, cx)));
    settle_board(&board, cx, |board| is_loaded(board, "SUB-2"));
    render(cx);
    assert_eq!(label(cx, "issue-back").as_deref(), Some("Back to issues"));
    press(cx, "escape");
    assert!(seen.borrow().is_empty());
}
