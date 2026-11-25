//! The turns close when the inspector hides, shows another tab, or the
//! ship leaves the live fleet, and stay closed when it comes back.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_client_fleet::{FleetState, Link};

use super::super::super::remove::settle;
use super::super::fleet::ship;
use super::super::live::{live, set_fleet};
use super::{is_open, opened};

#[gpui_kit::test]
fn hiding_the_inspector_or_another_tab_closes_the_turns(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    assert!(is_open(&board, cx));
    cx.update(|window, cx| window.click(("workspace-inspector-tab", 2usize), cx));
    settle(cx);
    assert!(!is_open(&board, cx), "the People tab");
    cx.update(|window, cx| window.click(("workspace-inspector-tab", 0usize), cx));
    settle(cx);
    cx.update(|window, cx| window.click("irc-inspector-activity", cx));
    settle(cx);
    assert!(is_open(&board, cx));
    board.update(cx, crate::dock::toggle_members);
    settle(cx);
    assert!(!is_open(&board, cx), "hidden");
    board.update(cx, crate::dock::toggle_members);
    settle(cx);
    assert!(!is_open(&board, cx), "shown again, the ship's card");
}

#[gpui_kit::test]
fn the_turns_close_once_the_ship_leaves_the_live_fleet(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    cx.update(|window, cx| window.click("irc-inspector-activity", cx));
    settle(cx);
    assert!(is_open(&board, cx));
    set_fleet(&hub, FleetState::Followed(Link::Live), cx);
    assert!(is_open(&board, cx), "news of the same ship keeps them");
    set_fleet(&hub, FleetState::NoToken, cx);
    assert!(!is_open(&board, cx), "no longer live");
    set_fleet(&hub, live(), cx);
    assert!(!is_open(&board, cx), "live again, the ship's card");
}
