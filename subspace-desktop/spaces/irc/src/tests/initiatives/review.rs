//! A running request's line still announced as status when it opens a
//! thread, and the Ship tab's In play here asking for its references when
//! the tab is chosen.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Role, TestAppContext};

use super::super::remove::settle;
use super::fleet::ship;
use super::thread::finished;
use crate::fixtures::channel;
use crate::thread::MESSAGE_TAGS;
use crate::tiles::TileRef;
use crate::view::InspectorTab;

#[gpui_kit::test]
fn a_linked_status_line_is_still_announced_as_status(cx: &mut TestAppContext) {
    let (_, _, cx) = finished(cx, &[MESSAGE_TAGS]);
    cx.update(|window, _| {
        assert_eq!(window.find("irc-status:ask1").role(), Some(Role::Button));
        assert_eq!(
            window.find("irc-status:ask1:live").role(),
            Some(Role::Status)
        );
    });
}

#[gpui_kit::test]
fn choosing_the_ship_tab_asks_for_what_in_play_lists(cx: &mut TestAppContext) {
    const KEY: &str = "acme/web#492";
    let (board, hub, cx) = ship(cx);
    board.update(cx, |board, cx| {
        board.inspector_tab = InspectorTab::People;
        cx.notify();
    });
    settle(cx);
    // A tile whose message is out of view: kept, but no row asked for it.
    hub.update(cx, |hub, _| {
        let room = channel("#aurora-lyra");
        let seq = hub
            .state
            .buffer(&room)
            .and_then(|held| held.entries.front())
            .map(|entry| entry.seq)
            .unwrap();
        let found = [(seq, vec![TileRef::PullRequest(KEY.into())])];
        hub.tiles.read(&room, u64::MAX, (&[seq], &found));
    });
    assert!(hub.read_with(cx, |hub, _| hub.pull_requests.state(KEY).is_none()));
    cx.update(|window, cx| window.click(("workspace-inspector-tab", 0usize), cx));
    settle(cx);
    assert!(
        hub.read_with(cx, |hub, _| hub.pull_requests.state(KEY).is_some()),
        "asked for as the tab shows"
    );
}
