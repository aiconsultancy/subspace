//! A clear's hold on the cleared token: only an operation queued with the
//! clear or after it ends it, so a save or read queued before the clear
//! never brings the token back when the keychain refuses the delete.

use gpui_kit::TestAppContext;
use subspace_framework::core::Secret;

use super::ordering::{Shelf, store};
use super::{HOME, aurora, service, state, token};
use crate::FleetState;

/// A save queued before a clear never ends the hold the clear set: its
/// write lands after the clear began, the delete is refused, and neither a
/// refused save nor removing and adding the fleet again brings it back.
#[gpui_kit::test(iterations = 20)]
fn a_save_queued_before_a_refused_clear_never_ends_its_hold(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let service = service(store(&shelf), aurora(HOME), cx);
    shelf.with(|held| held.fail_deletes = true);
    service.update(cx, |service, cx| {
        service
            .save_token("aurora", Secret::new("earlier"), cx)
            .detach();
        service.clear_token("aurora", cx).detach();
    });
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::ClearFailed);
    assert!(!shelf.empty(), "the refused delete left it in the keychain");
    shelf.with(|held| held.fail_saves = true);
    service.update(cx, |service, cx| {
        service
            .save_token("aurora", Secret::new("refused"), cx)
            .detach();
    });
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::ClearFailed);
    assert_eq!(
        token(&service, cx),
        None,
        "a refused save does not bring it back"
    );
    service.update(cx, |service, cx| service.set_fleets(Vec::new(), cx));
    cx.run_until_parked();
    service.update(cx, |service, cx| service.set_fleets(aurora(HOME), cx));
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::ClearFailed);
    assert_eq!(
        token(&service, cx),
        None,
        "nor removing and adding the fleet"
    );
    let shared = service.read_with(cx, |service, _| service.shared_fleets());
    assert!(
        shared.iter().all(|fleet| fleet.token.is_none()),
        "nor shared"
    );
}

/// A read queued before a clear that finds no token never ends the hold:
/// a save between it and the clear wrote the token the clear then could
/// not delete.
#[gpui_kit::test(iterations = 20)]
fn a_read_queued_before_a_refused_clear_never_ends_its_hold(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let service = service(store(&shelf), aurora(HOME), cx);
    shelf.with(|held| held.fail_deletes = true);
    service.update(cx, |service, cx| {
        service.load_token("aurora".to_owned(), cx);
        service
            .save_token("aurora", Secret::new("earlier"), cx)
            .detach();
        service.clear_token("aurora", cx).detach();
    });
    cx.run_until_parked();
    shelf.with(|held| held.fail_saves = true);
    service.update(cx, |service, cx| {
        service
            .save_token("aurora", Secret::new("refused"), cx)
            .detach();
    });
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::ClearFailed);
    assert_eq!(token(&service, cx), None);
}
