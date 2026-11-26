//! A running request's line still announced as status when it opens a
//! thread, and the Ship tab's In play here asking for its references when
//! the tab is chosen, and only while the Space shows a ship's Ship tab.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Role, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Event};

use super::super::inject;
use super::super::remove::settle;
use super::fleet::ship;
use super::thread::finished;
use crate::Irc;
use crate::fixtures::{channel, message};
use crate::hub::Hub;
use crate::thread::MESSAGE_TAGS;
use crate::tiles::TileRef;
use crate::view::{self, InspectorTab};

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

const IN_PLAY: &str = "acme/web#492";

/// A tile for [`IN_PLAY`] kept by `room`'s first message, as if that
/// message were out of view, so no row asks for it; then the board syncs.
fn tile_out_of_view(
    (board, hub): (&Entity<Irc>, &Entity<Hub>),
    room: &BufferKey,
    cx: &mut VisualTestContext,
) {
    hub.update(cx, |hub, _| {
        let seq = hub
            .state
            .buffer(room)
            .and_then(|held| held.entries.front())
            .map(|entry| entry.seq)
            .unwrap();
        let found = [(seq, vec![TileRef::PullRequest(IN_PLAY.into())])];
        hub.tiles.read(room, u64::MAX, (&[seq], &found));
    });
    board.update(cx, view::sync);
    settle(cx);
}

fn asked(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> bool {
    hub.read_with(cx, |hub, _| hub.pull_requests.state(IN_PLAY).is_some())
}

#[gpui_kit::test]
fn an_ordinary_channel_never_asks_for_what_it_has_in_play(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let lobby = channel("#lobby");
    inject(
        &hub,
        vec![
            Event::Joined {
                buffer: lobby.clone(),
            },
            Event::Line(message(&lobby, "mira", "Morning.", 4)),
        ],
        cx,
    );
    board.update(cx, |board, cx| {
        view::reveal(board, &lobby, cx);
        board.inspector_tab = InspectorTab::Subject;
    });
    settle(cx);
    tile_out_of_view((&board, &hub), &lobby, cx);
    assert!(!asked(&hub, cx), "no Ship tab shows it");
}

#[gpui_kit::test]
fn an_inactive_view_never_asks_for_what_is_in_play(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    board.update(cx, |board, _| {
        board.inspector_tab = InspectorTab::Subject;
        board.active = false;
    });
    tile_out_of_view((&board, &hub), &channel("#aurora-lyra"), cx);
    assert!(!asked(&hub, cx), "nothing shows it");
    board.update(cx, |board, cx| {
        board.active = true;
        view::sync(board, cx);
    });
    settle(cx);
    assert!(asked(&hub, cx), "asked once the view is in front");
}

#[gpui_kit::test]
fn refreshing_a_pull_request_keeps_its_ship_inspector_row(cx: &mut TestAppContext) {
    use subspace_framework::contracts::github::{
        PullRequestDetails, PullRequestStatus, Resolution, ResolutionState,
    };
    const ROW: &str = "irc-inspector-play:acme/web#492";
    let (board, hub, cx) = ship(cx);
    tile_out_of_view((&board, &hub), &channel("#aurora-lyra"), cx);
    let summary = PullRequestDetails::new(
        IN_PLAY,
        "Keep viewport updates stable",
        "https://github.com/acme/web/pull/492",
        PullRequestStatus::Merged,
    );
    let show = |state: ResolutionState, cx: &mut VisualTestContext| {
        hub.update(cx, |hub, cx| {
            hub.pull_requests
                .record(vec![Resolution::new(IN_PLAY, state)]);
            cx.notify();
        });
        settle(cx);
    };
    show(ResolutionState::Available(summary), cx);
    let label = |cx: &mut VisualTestContext| {
        cx.update(|window, _| window.find(ROW).label().unwrap().to_owned())
    };
    let before = label(cx);
    assert_eq!(before, "acme/web#492 Keep viewport updates stable Merged");

    show(ResolutionState::Pending, cx);
    assert_eq!(label(cx), before);
}
