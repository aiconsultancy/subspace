//! Preview rendering, shared navigation, and scope changes through the fixture broker.

mod variants;

use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px, size};
use subspace_framework::contracts::issues::{Changed, CurrentScope, IssueScope, ResolutionState};
use subspace_framework::dashboard::{DashboardEvent, contracts};

use super::{TRACKER, open_board, open_room};

#[gpui_kit::test]
fn available_issues_show_one_tile_per_message_and_navigate_to_the_responder(
    cx: &mut TestAppContext,
) {
    const TILE: &str = "irc-line:2:issue-preview:TST-12";
    let text = "TST-12 [same issue](https://linear.example.com/workspace/issue/TST-12/fixture) TST-13 `see TST-14`";
    let (board, _, tracker, cx) = open_board(cx, true, text);
    let events = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|_, cx| {
        let sink = events.clone();
        cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Navigate(link) = event {
                sink.borrow_mut().push(link.clone());
            }
        })
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_all(TILE).len(), 1);
        assert!(
            window
                .find_all("irc-line:2:issue-preview:TST-13")
                .is_empty()
        );
        window.click(TILE, cx);
    });
    cx.run_until_parked();
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].url,
        "https://api.example.test/api/v1/issues/TST-12?profile=dev"
    );
    assert_eq!(
        events[0].responder,
        contracts::MessageKey::request::<CurrentScope>()
    );
    assert_eq!(
        cx.update(|_, cx| tracker.unwrap().read(cx).asked.clone()),
        [vec!["TST-12".to_owned(), "TST-13".to_owned()]],
    );
}

#[gpui_kit::test]
fn changing_scope_removes_tiles_and_keeps_the_message(cx: &mut TestAppContext) {
    const TILE: &str = "irc-line:2:issue-preview:TST-12";
    let (_, tracker, cx) = open_room(cx, true, "TST-12 and its link");
    let tracker = tracker.unwrap();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_all(TILE).len(), 1);
        tracker.update(cx, |tracker, _| {
            let next = IssueScope::new("dev", "Subspace API", "https://other-api.example.test/api");
            tracker.scope = Some(next.with_project_keys(["TST"]));
            tracker
                .answers
                .insert("TST-12".into(), ResolutionState::Unavailable);
        });
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find_all(TILE).is_empty());
        assert_eq!(
            window.find("irc-line:2:row").label(),
            Some("bob: TST-12 and its link")
        );
    });
}

#[gpui_kit::test]
fn withdrawing_scope_removes_tiles_from_the_visible_transcript(cx: &mut TestAppContext) {
    const TILE: &str = "irc-line:2:issue-preview:TST-12";
    let (hub, tracker, cx) = open_room(cx, true, "TST-12 and its link");
    let tracker = tracker.unwrap();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_all(TILE).len(), 1);
        tracker.update(cx, |tracker, _| tracker.scope = None);
        assert!(contracts::publish(TRACKER, Changed, cx).is_ok());
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(hub.read(cx).issues.state("TST-12").is_none());
        assert!(window.find_all(TILE).is_empty());
        assert_eq!(
            window.find("irc-line:2:row").label(),
            Some("bob: TST-12 and its link")
        );
    });
}

#[gpui_kit::test]
fn previews_stay_within_a_narrow_transcript(cx: &mut TestAppContext) {
    const TILE: &str = "irc-line:2:issue-preview:TST-18";
    let (board, hub, _, cx) = open_board(cx, true, "TST-18");
    let room = crate::fixtures::channel("#subspace");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(matches!(
            hub.read(cx).issues.state("TST-18"),
            Some(ResolutionState::Available(_))
        ));
        assert!(
            !window.find_all(TILE).is_empty(),
            "available before resizing"
        );
        window.resize(size(px(860.), px(960.)));
        let conversation = board.read(cx).dock.open[&room].entity.clone();
        conversation.update(cx, |conversation, cx| assert!(conversation.reveal(2, cx)));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let transcript = window.find("irc-transcript-pane").bounds();
        assert_eq!(
            window.find_all(TILE).len(),
            1,
            "transcript bounds: {transcript:?}"
        );
        let tile = window.find(TILE).bounds();
        assert!(tile.right() <= transcript.right());
        assert!(
            tile.size.height < px(40.),
            "a single issue keeps the compact chip height"
        );
    });
}
