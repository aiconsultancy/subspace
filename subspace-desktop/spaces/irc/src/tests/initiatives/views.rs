//! The fleets a view shows, as the hub and the inspector hold them: a
//! hidden view never empties what a shown one publishes, a view that shows
//! no fleet never starts the fleet service.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_fleet::{FleetApi, FleetRecord, TokenStore};

use super::super::remove::settle;
use super::fleet::ship;
use crate::hub::Hub;

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
