use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Bounds, Pixels, TestAppContext, VisualTestContext};
use subspace_client_fleet::{FleetState, Link};
use subspace_client_irc::Event;

use super::{channel, inject, live, message, set_fleet, set_snapshot, settle, ship, tests};

#[gpui_kit::test]
fn the_ship_waits_for_crew_and_keeps_complete_details_during_refresh(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    let room = channel("#aurora-lyra");
    inject(
        &hub,
        vec![Event::Line(message(
            &room,
            "mira",
            "Review https://github.com/acme/web/pull/492",
            4,
        ))],
        cx,
    );
    let mut snapshot = tests::snapshot();
    let detail = snapshot.ships.remove("aurora-lyra").unwrap();
    set_snapshot(&hub, live(), &snapshot, cx);
    cx.update(|window, _| {
        assert!(
            window
                .find("irc-inspector-reason")
                .label()
                .unwrap()
                .starts_with("Loading ship details")
        );
        assert!(window.try_find("irc-inspector-ship").is_none());
        assert!(window.try_find("irc-inspector-play:acme/web#492").is_none());
    });
    snapshot.ships.insert("aurora-lyra".into(), detail);
    set_snapshot(&hub, live(), &snapshot, cx);
    let before = ship_rows(cx);

    snapshot.overview.as_mut().unwrap().ships[0].available = false;
    set_snapshot(&hub, live(), &snapshot, cx);
    assert_eq!(
        ship_rows(cx),
        before,
        "an unavailable sample keeps the last complete read"
    );
    set_fleet(&hub, FleetState::Followed(Link::Reconnecting), cx);
    assert_eq!(ship_rows(cx), before, "reconnecting keeps the panel steady");

    cx.executor().advance_clock(Duration::from_secs(5));
    settle(cx);
    cx.update(|window, _| assert!(window.find("irc-inspector-last-known").visible()));
    set_fleet(&hub, live(), cx);
    assert_eq!(ship_rows(cx), before);
    set_fleet(&hub, FleetState::Followed(Link::TokenRefused), cx);
    cx.update(|window, _| assert!(window.try_find("irc-inspector-ship").is_none()));
}

fn ship_rows(cx: &mut VisualTestContext) -> Vec<Bounds<Pixels>> {
    cx.update(|window, _| {
        assert!(window.try_find("irc-inspector-reason").is_none());
        [
            "irc-inspector-ship",
            "irc-inspector-crew:0",
            "irc-inspector-play:acme/web#492",
        ]
        .map(|id| window.find(id).bounds())
        .to_vec()
    })
}
