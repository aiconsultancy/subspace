//! The fleets a view shows, as the hub and the inspector hold them: a
//! hidden view never empties what a shown one publishes, a view that shows
//! no fleet never starts the fleet service, and every inspector tab starts
//! at the inspector body's text edge.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_fleet::{FleetApi, FleetRecord, TokenStore};

use super::super::remove::settle;
use super::fleet::ship;
use super::live::{live, set_fleet};
use crate::hub::Hub;
use crate::view;

#[gpui_kit::test]
fn a_hidden_view_on_the_same_hub_never_empties_the_fleets_a_shown_one_shows(
    cx: &mut TestAppContext,
) {
    let record = FleetRecord::new("aurora", "Aurora", Some("http://127.0.0.1:9".into()));
    cx.update(|cx| FleetApi::install_new(TokenStore::memory(), vec![record], cx));
    cx.run_until_parked();
    let (board, hub, cx) = ship(cx);
    board.update(cx, crate::live::watch::sync);
    settle(cx);
    let shown = |hub: &Entity<Hub>, cx: &mut VisualTestContext| {
        hub.read_with(cx, |hub, _| hub.fleets.contains_key("aurora"))
    };
    assert!(shown(&hub, cx));
    // A second view on the same hub, hidden, shows no fleet and publishes
    // its share: empty. (Two whole boards in one test window would trade
    // their persisted dock state back and forth, so the share goes in as
    // the view gives it.)
    hub.update(cx, |hub, cx| {
        crate::live::watch::share(hub, u64::MAX, crate::live::Lives::default(), cx);
    });
    settle(cx);
    assert!(shown(&hub, cx), "the hidden view publishes nothing over it");
    // The shown view closes: its share goes with it.
    board.update_in(cx, |board, window, cx| {
        subspace_framework::dashboard::Dashboard::deactivate(board, window, cx);
    });
    settle(cx);
    assert!(!shown(&hub, cx), "nothing shows the fleet any more");
}

#[gpui_kit::test]
fn a_view_that_shows_no_fleet_never_starts_the_fleet_service(cx: &mut TestAppContext) {
    let (board, _hub, cx) = crate::tests::mount(cx, crate::tests::settings());
    // A service a test installs stands for the one the app would start.
    let record = FleetRecord::new("aurora", "Aurora", None);
    cx.update(|_, cx| FleetApi::install_new(TokenStore::memory(), vec![record], cx));
    board.update(cx, crate::live::watch::sync);
    settle(cx);
    assert!(
        board.read_with(cx, |board, _| !crate::live::watch::bound(&board.watch)),
        "no fleet on show: the service is not reached"
    );
}

#[gpui_kit::test]
fn every_inspector_tab_starts_its_content_at_the_body_s_text_edge(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    let (body, edge) = cx.update(|window, cx| {
        let body = window.find("workspace-inspector-body").bounds();
        let tokens = subspace_framework::dashboard::Tokens::current(cx);
        (
            body,
            body.origin.x + subspace_framework::dashboard::workspace::text_edge(&tokens),
        )
    });
    let place = |id: &'static str, cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            let bounds = window.find(id).bounds();
            (bounds.origin.x, bounds.origin.y)
        })
    };
    let (ship_x, ship_y) = place("irc-inspector-subject", cx);
    assert_eq!(ship_x, edge, "the Ship tab adds no inset of its own");
    for (tab, id) in [
        (view::InspectorTab::Thread, "irc-thread-prompt-line"),
        (view::InspectorTab::People, "irc-members"),
    ] {
        board.update(cx, |board, cx| {
            board.inspector_tab = tab;
            cx.notify();
        });
        settle(cx);
        let (x, y) = place(id, cx);
        assert_eq!(x, edge, "{id} starts at the same edge");
        assert_eq!(y, ship_y, "{id} starts at the same top");
    }
    assert!(ship_y > body.origin.y);
}
