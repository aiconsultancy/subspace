//! Keyboard and pointer navigation of the Ship tab's crew detail.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use subspace_client_fleet::{FleetApi, TokenStore};
use subspace_client_irc::Event;

use super::super::{inject, remove::settle};
use super::fleet::ship;
use super::live::{live, set_fleet};
use crate::{fixtures::channel, view};

const ROW: &str = "irc-inspector-crew:wren";
const BACK: &str = "irc-crew-back";
const SHIP: (&str, usize) = ("workspace-inspector-tab", 0);

fn install_fleet_api(cx: &mut VisualTestContext) {
    cx.update(|_, cx| FleetApi::install_new(TokenStore::memory(), vec![], cx));
}

pub(super) fn round_trip(cx: &mut VisualTestContext) {
    install_fleet_api(cx);
    cx.update(|window, cx| {
        window.click(SHIP, cx);
        for _ in 0..20 {
            if window.find(ROW).focused() == Some(true) {
                break;
            }
            window.focus_next(cx);
            window.render_frame(cx);
        }
        assert_eq!(window.find(ROW).focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(window.find(BACK).label(), Some("Back to Ship"));
        assert_eq!(window.find(BACK).focused(), Some(true));
        window.press("space", cx);
        assert_eq!(window.find(ROW).focused(), Some(true));
        window.click(ROW, cx);
        assert!(window.find(BACK).visible());
        window.click(BACK, cx);
        assert!(window.find(ROW).visible());
    });
}

#[gpui_kit::test]
fn back_to_ship_focuses_the_ship_tab_when_no_crew_row_remains(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.update(|window, cx| window.click(ROW, cx));
    let mut snapshot = crate::live::tests::snapshot();
    snapshot
        .ships
        .get_mut("aurora-lyra")
        .unwrap()
        .crew_members
        .clear();
    hub.update(cx, |hub, cx| {
        hub.fleets = [(
            "aurora".to_owned(),
            crate::live::FleetLive::new(live(), Some(&snapshot)),
        )]
        .into();
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find(BACK).focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(window.find(SHIP).focused(), Some(true));
        assert!(window.try_find(ROW).is_none());
    });
}

#[gpui_kit::test]
fn focusing_another_buffer_clears_crew_detail(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.update(|window, cx| window.click(ROW, cx));
    let other = channel("#aurora-starbase");
    inject(
        &hub,
        vec![Event::Joined {
            buffer: other.clone(),
        }],
        cx,
    );
    board.update(cx, |board, cx| {
        assert!(board.inspector_crew.is_some());
        view::reveal(board, &other, cx);
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&other));
        assert!(board.inspector_crew.is_none());
    });
}
